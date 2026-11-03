use crate::prelude::*;
use crate::*;

pub(crate) fn open_account_setup_modal(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    app_cx: &mut App,
) {
    state_entity.update(app_cx, |state, cx| {
        state.modals.account_setup_open = true;
        state.modals.settings_open = false;
        state.modals.advanced_search_open = false;
        cx.notify();
    });
    modal_entity.update(app_cx, |modal, cx| {
        modal.reset(cx);
    });
}

pub(crate) fn close_account_setup_modal(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        if !state.accounts.is_empty() {
            state.modals.account_setup_open = false;
        }
        cx.notify();
    });
}

pub(crate) fn handle_account_setup_key(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    key: &str,
    app_cx: &mut App,
) -> bool {
    match key {
        "escape" => {
            let (mode, busy, first_run) = {
                let modal = modal_entity.read(app_cx);
                (
                    modal.mode,
                    modal.is_busy(),
                    modal.state.read(app_cx).accounts.is_empty(),
                )
            };
            if busy {
                return true;
            }
            if mode != AccountSetupMode::Choice {
                modal_entity.update(app_cx, |modal, cx| {
                    modal.show_mode(AccountSetupMode::Choice, cx);
                });
                return true;
            }
            if !first_run {
                close_account_setup_modal(state_entity, app_cx);
            }
            true
        }
        "enter" | "return" => {
            let mode = modal_entity.read(app_cx).mode;
            if mode == AccountSetupMode::ManualImap
                || mode == AccountSetupMode::MicrosoftAppPassword
            {
                start_account_setup_create(state_entity, modal_entity, app_cx);
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

pub(crate) fn cancel_account_setup_form(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    first_run: bool,
    app_cx: &mut App,
) {
    if first_run {
        modal_entity.update(app_cx, |modal, cx| {
            modal.show_mode(AccountSetupMode::Choice, cx);
        });
    } else {
        close_account_setup_modal(state_entity, app_cx);
    }
}

pub(crate) fn start_account_setup_oauth(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    provider: AccountSetupProvider,
    app_cx: &mut App,
) {
    if modal_entity.read(app_cx).is_busy() {
        return;
    }

    let base_url = state_entity.read(app_cx).api_base_url.clone();
    modal_entity.update(app_cx, |modal, cx| {
        modal.oauth_loading = Some(provider);
        modal.status = None;
        cx.notify();
    });
    state_entity.update(app_cx, |state, cx| {
        state.last_error = None;
        cx.notify();
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { authorize_account_setup(base_url, provider) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let status = match &result {
                    Ok(_) => (
                        format!("Browser opened for {} authorization.", provider.label()),
                        true,
                    ),
                    Err(err) => (err.to_string(), false),
                };

                modal_entity.update(&mut async_cx, |modal, cx| {
                    modal.set_status(Some(status.clone()), cx);
                });

                if !status.1 {
                    state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(status.0);
                        cx.notify();
                    });
                }
            }
        })
        .detach();
}

pub(crate) fn start_account_setup_create(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    app_cx: &mut App,
) {
    let (mode, draft, busy) = {
        let modal = modal_entity.read(app_cx);
        (
            modal.mode,
            AccountSetupFormDraft {
                name: modal.name_input.read(app_cx).text().to_string(),
                email: modal.email_input.read(app_cx).text().to_string(),
                imap_host: modal.imap_host_input.read(app_cx).text().to_string(),
                imap_port: modal.imap_port_input.read(app_cx).text().to_string(),
                imap_password: modal.imap_password_input.read(app_cx).text().to_string(),
                smtp_host: modal.smtp_host_input.read(app_cx).text().to_string(),
                smtp_port: modal.smtp_port_input.read(app_cx).text().to_string(),
                smtp_password: modal.smtp_password_input.read(app_cx).text().to_string(),
                same_password: modal.same_password,
            },
            modal.is_busy(),
        )
    };
    if busy {
        return;
    }

    let request = match mode {
        AccountSetupMode::ManualImap => build_manual_account_request(draft),
        AccountSetupMode::MicrosoftAppPassword => build_microsoft_app_password_request(draft),
        AccountSetupMode::Choice => Err("Choose an account setup method.".to_string()),
    };
    let request = match request {
        Ok(request) => request,
        Err(message) => {
            modal_entity.update(app_cx, |modal, cx| {
                modal.set_status(Some((message.clone(), false)), cx);
            });
            state_entity.update(app_cx, |state, cx| {
                state.last_error = Some(message);
                cx.notify();
            });
            return;
        }
    };

    let base_url = state_entity.read(app_cx).api_base_url.clone();
    modal_entity.update(app_cx, |modal, cx| {
        modal.submitting = true;
        modal.status = None;
        cx.notify();
    });
    state_entity.update(app_cx, |state, cx| {
        state.last_error = None;
        cx.notify();
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { create_account_and_reload(base_url, request) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_account_create_reload(state, loaded);
                        }
                        Err(err) => {
                            state.last_error = Some(err.to_string());
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                });

                modal_entity.update(&mut async_cx, |modal, cx| {
                    if let Some(message) = error_message {
                        modal.set_status(Some((message, false)), cx);
                    } else {
                        modal.reset(cx);
                    }
                });
            }
        })
        .detach();
}

pub(crate) fn start_account_setup_refresh(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    app_cx: &mut App,
) {
    if modal_entity.read(app_cx).is_busy() {
        return;
    }

    let base_url = state_entity.read(app_cx).api_base_url.clone();
    modal_entity.update(app_cx, |modal, cx| {
        modal.refreshing = true;
        modal.status = None;
        cx.notify();
    });
    state_entity.update(app_cx, |state, cx| {
        state.last_error = None;
        cx.notify();
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { reload_accounts(base_url) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());
                let loaded_has_accounts = result
                    .as_ref()
                    .map(|loaded| !loaded.accounts.is_empty())
                    .unwrap_or(false);

                if loaded_has_accounts {
                    state_entity.update(&mut async_cx, |state, cx| {
                        if let Ok(loaded) = result {
                            apply_account_create_reload(state, loaded);
                            sync_window_title(state, cx);
                            cx.notify();
                        }
                    });
                }

                modal_entity.update(&mut async_cx, |modal, cx| {
                    if loaded_has_accounts {
                        modal.reset(cx);
                    } else if let Some(message) = error_message.clone() {
                        modal.set_status(Some((message.clone(), false)), cx);
                    } else {
                        modal.set_status(
                            Some(("No authorized account found yet.".to_string(), false)),
                            cx,
                        );
                    }
                });

                if let Some(message) = error_message {
                    state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(message);
                        cx.notify();
                    });
                }
            }
        })
        .detach();
}

pub(crate) fn authorize_account_setup(
    base_url: String,
    provider: AccountSetupProvider,
) -> Result<String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        let response = match provider {
            AccountSetupProvider::Gmail => client.gmail_authorize().await?,
            AccountSetupProvider::Microsoft => client.microsoft_authorize().await?,
        };
        open_url_in_browser(&response.url)
            .with_context(|| format!("open {} authorization URL", provider.label()))?;
        Ok(response.url)
    })
}

pub(crate) fn create_account_and_reload(
    base_url: String,
    request: CreateAccount,
) -> Result<NativeAppState> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client.create_account(&request).await.with_context(|| {
            format!(
                "create {} account for {}",
                request.provider_type, request.email
            )
        })?;
        let mut loaded = load_accounts(&client)
            .await
            .context("reload accounts after creating account")?;
        loaded.api_base_url = base_url;
        Ok(loaded)
    })
}

pub(crate) fn reload_accounts(base_url: String) -> Result<NativeAppState> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        let mut loaded = load_accounts(&client).await.context("reload accounts")?;
        loaded.api_base_url = base_url;
        Ok(loaded)
    })
}

pub(crate) fn delete_account_and_reload(
    base_url: String,
    account_id: String,
) -> Result<NativeAppState> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .delete_account(&account_id)
            .await
            .with_context(|| format!("delete account {account_id}"))?;
        let mut loaded = load_accounts(&client)
            .await
            .with_context(|| "reload accounts after deleting account")?;
        loaded.api_base_url = base_url;
        Ok(loaded)
    })
}

pub(crate) fn apply_account_create_reload(state: &mut NativeAppState, mut loaded: NativeAppState) {
    let had_accounts = !state.accounts.is_empty();
    let theme = state.theme.clone();
    let density = state.density.clone();
    let layout = state.layout;
    let selected_view = state.selected_view.clone();
    let selected_folder = state.selected_folder.clone();
    let selected_label = state.selected_label.clone();
    let selected_message = state.selected_message.clone();
    let selected_message_ids = state.selected_message_ids.clone();
    let selected_full_message = state.selected_full_message.clone();
    let selected_message_resource = state.selected_message_resource.clone();
    let selected_message_labels = state.selected_message_labels.clone();
    let selected_message_labels_resource = state.selected_message_labels_resource.clone();
    let selected_event = state.selected_event.clone();
    let calendar_week_start = state.calendar_week_start;
    let calendar_events = state.calendar_events.clone();
    let calendar_events_resource = state.calendar_events_resource.clone();
    let calendar_event_links_event = state.calendar_event_links_event.clone();
    let calendar_event_links = state.calendar_event_links.clone();
    let calendar_event_links_resource = state.calendar_event_links_resource.clone();
    let search = state.search.clone();
    let search_event_results = state.search_event_results.clone();
    let messages = state.messages.clone();
    let messages_resource = state.messages_resource.clone();
    let palette = state.palette.clone();
    let sync = state.sync.clone();
    let compose = state.compose.clone();

    loaded.theme = theme;
    loaded.density = density;
    loaded.layout = layout;
    if had_accounts {
        loaded.selected_view = selected_view;
        loaded.selected_folder = selected_folder;
        loaded.selected_label = selected_label;
        loaded.selected_message = selected_message;
        loaded.selected_message_ids = selected_message_ids;
        loaded.selected_full_message = selected_full_message;
        loaded.selected_message_resource = selected_message_resource;
        loaded.selected_message_labels = selected_message_labels;
        loaded.selected_message_labels_resource = selected_message_labels_resource;
        loaded.selected_event = selected_event;
        loaded.calendar_week_start = calendar_week_start;
        loaded.calendar_events = calendar_events;
        loaded.calendar_events_resource = calendar_events_resource;
        loaded.calendar_event_links_event = calendar_event_links_event;
        loaded.calendar_event_links = calendar_event_links;
        loaded.calendar_event_links_resource = calendar_event_links_resource;
        loaded.search = search;
        loaded.search_event_results = search_event_results;
        loaded.messages = messages;
        loaded.messages_resource = messages_resource;
    } else {
        loaded.selected_view = SelectedView::Mail;
    }
    loaded.palette = palette;
    loaded.sync = sync;
    loaded.compose = compose;
    loaded.modals = ModalStates {
        account_setup_open: false,
        settings_open: false,
        advanced_search_open: false,
    };
    loaded.last_error = None;
    *state = loaded;
}

pub(crate) fn apply_initial_account_load(
    state: &mut NativeAppState,
    accounts_request: ResourceRequest,
    messages_request: ResourceRequest,
    mut loaded: NativeAppState,
) -> bool {
    if !state
        .accounts_resource
        .resolve(accounts_request, loaded.accounts.clone())
    {
        return false;
    }
    if !state
        .messages_resource
        .resolve(messages_request, loaded.messages.clone())
    {
        return false;
    }

    let theme = state.theme.clone();
    let density = state.density.clone();
    let layout = state.layout;
    let palette = state.palette.clone();
    let sync = state.sync.clone();

    loaded.theme = theme;
    loaded.density = density;
    loaded.layout = layout;
    loaded.palette = palette;
    loaded.sync = sync;
    loaded.last_error = None;
    *state = loaded;
    true
}

pub(crate) fn reject_initial_account_load(
    state: &mut NativeAppState,
    accounts_request: ResourceRequest,
    messages_request: ResourceRequest,
    err: anyhow::Error,
) -> bool {
    let message = err.to_string();
    let accounts_rejected = state
        .accounts_resource
        .reject(accounts_request, message.clone());
    let messages_rejected = state
        .messages_resource
        .reject(messages_request, message.clone());
    if !accounts_rejected && !messages_rejected {
        return false;
    }

    state.last_error = Some(message);
    if state
        .last_error
        .as_deref()
        .is_some_and(|message| message.contains("API error 401"))
    {
        state.auth_required = true;
        clear_native_auth_token();
    }
    true
}

pub(crate) fn apply_account_delete_reload(
    state: &mut NativeAppState,
    deleted_account_id: &str,
    mut loaded: NativeAppState,
) {
    let theme = state.theme.clone();
    let density = state.density.clone();
    let layout = state.layout;
    let selected_view = state.selected_view.clone();
    let selected_event = state.selected_event.clone();
    let calendar_week_start = state.calendar_week_start;
    let calendar_events = state.calendar_events.clone();
    let calendar_events_resource = state.calendar_events_resource.clone();
    let calendar_event_links_event = state.calendar_event_links_event.clone();
    let calendar_event_links = state.calendar_event_links.clone();
    let calendar_event_links_resource = state.calendar_event_links_resource.clone();
    let palette = state.palette.clone();
    let sync = state.sync.clone();
    let account_setup_open = state.modals.account_setup_open;
    let compose = if state.compose.account_id.as_deref() == Some(deleted_account_id) {
        ComposeState {
            revision: state.compose.revision.saturating_add(1),
            ..ComposeState::default()
        }
    } else {
        state.compose.clone()
    };

    loaded.theme = theme;
    loaded.density = density;
    loaded.layout = layout;
    loaded.selected_view = selected_view;
    loaded.selected_event = selected_event;
    loaded.calendar_week_start = calendar_week_start;
    loaded.calendar_events = calendar_events;
    loaded.calendar_events_resource = calendar_events_resource;
    loaded.calendar_event_links_event = calendar_event_links_event;
    loaded.calendar_event_links = calendar_event_links;
    loaded.calendar_event_links_resource = calendar_event_links_resource;
    loaded.palette = palette;
    loaded.sync = sync;
    loaded.compose = compose;
    loaded.modals = ModalStates {
        account_setup_open,
        settings_open: false,
        advanced_search_open: false,
    };
    loaded.last_error = None;
    *state = loaded;
}

pub(crate) fn build_manual_account_request(
    draft: AccountSetupFormDraft,
) -> std::result::Result<CreateAccount, String> {
    let name = validate_account_setup_name(&draft.name)?;
    let email = validate_account_setup_email(&draft.email)?;
    let imap_host = validate_account_setup_host(&draft.imap_host, "IMAP host")?;
    let imap_port = parse_account_setup_port(&draft.imap_port, "IMAP")?;
    let smtp_host = validate_account_setup_host(&draft.smtp_host, "SMTP host")?;
    let smtp_port = parse_account_setup_port(&draft.smtp_port, "SMTP")?;
    let imap_password = validate_account_setup_password(&draft.imap_password, "IMAP password")?;
    let smtp_password = if draft.same_password {
        imap_password.clone()
    } else {
        validate_account_setup_password(&draft.smtp_password, "SMTP password")?
    };

    Ok(CreateAccount {
        name,
        email,
        provider_type: "imap".to_string(),
        auth_type: "basic".to_string(),
        oauth_token_json: None,
        host: Some(imap_host),
        port: Some(imap_port),
        use_ssl: None,
        password: Some(imap_password),
        smtp_host: Some(smtp_host),
        smtp_port: Some(smtp_port),
        smtp_password: Some(smtp_password),
    })
}

pub(crate) fn build_microsoft_app_password_request(
    draft: AccountSetupFormDraft,
) -> std::result::Result<CreateAccount, String> {
    let name = validate_account_setup_name(&draft.name)?;
    let email = validate_account_setup_email(&draft.email)?;
    let imap_host = validate_account_setup_host(&draft.imap_host, "IMAP host")?;
    let imap_port = parse_account_setup_port(&draft.imap_port, "IMAP")?;
    let smtp_host = validate_account_setup_host(&draft.smtp_host, "SMTP host")?;
    let smtp_port = parse_account_setup_port(&draft.smtp_port, "SMTP")?;
    let password = validate_account_setup_password(&draft.imap_password, "App password")?;

    Ok(CreateAccount {
        name,
        email,
        provider_type: "microsoft365".to_string(),
        auth_type: "app_password".to_string(),
        oauth_token_json: None,
        host: Some(imap_host),
        port: Some(imap_port),
        use_ssl: None,
        password: Some(password.clone()),
        smtp_host: Some(smtp_host),
        smtp_port: Some(smtp_port),
        smtp_password: Some(password),
    })
}

pub(crate) fn validate_account_setup_name(value: &str) -> std::result::Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err("Enter a display name.".to_string())
    } else {
        Ok(value)
    }
}

pub(crate) fn validate_account_setup_email(value: &str) -> std::result::Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err("Enter an email address.".to_string());
    }
    if !value.contains('@') {
        return Err("Enter a valid email address.".to_string());
    }
    Ok(value)
}

pub(crate) fn validate_account_setup_host(
    value: &str,
    label: &'static str,
) -> std::result::Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(format!("Enter a {label}."))
    } else {
        Ok(value)
    }
}

pub(crate) fn validate_account_setup_password(
    value: &str,
    label: &'static str,
) -> std::result::Result<String, String> {
    if value.trim().is_empty() {
        Err(format!("Enter a {label}."))
    } else {
        Ok(value.to_string())
    }
}

pub(crate) fn parse_account_setup_port(
    value: &str,
    label: &'static str,
) -> std::result::Result<i64, String> {
    let port = value
        .trim()
        .parse::<i64>()
        .map_err(|_| format!("{label} port must be a positive whole number."))?;
    if !(1..=65535).contains(&port) {
        return Err(format!("{label} port must be between 1 and 65535."));
    }
    Ok(port)
}
