use crate::prelude::*;
use crate::*;

pub(crate) fn start_new_compose(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    let load = state_entity.update(app_cx, |state, cx| {
        let Some(account_id) = state.accounts.first().map(|account| account.id.clone()) else {
            let message = "Add an account before composing a message.".to_string();
            state.compose.error = Some(message.clone());
            state.last_error = Some(message);
            cx.notify();
            return None;
        };

        state.compose = ComposeState {
            open: true,
            mode: Some("compose".to_string()),
            account_id: Some(account_id.clone()),
            revision: state.compose.revision.saturating_add(1),
            signature_loading: true,
            ..ComposeState::default()
        };
        state.selected_view = SelectedView::Mail;
        state.last_error = None;
        cx.notify();
        Some((
            state.api_base_url.clone(),
            account_id,
            state.compose.revision,
        ))
    });

    if let Some((base_url, account_id, revision)) = load {
        start_compose_signature_load(state_entity, base_url, account_id, revision, app_cx);
    }
}

pub(crate) fn start_reply_draft(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    let load = state_entity.update(app_cx, |state, cx| {
        let Some(seed) = selected_reply_seed(state) else {
            let message = "Select a message before replying.".to_string();
            state.compose.error = Some(message.clone());
            state.last_error = Some(message);
            cx.notify();
            return None;
        };
        let Some(account_id) = selected_account_id(state) else {
            let message = "No account is available for this message.".to_string();
            state.compose.error = Some(message.clone());
            state.last_error = Some(message);
            cx.notify();
            return None;
        };

        state.compose = ComposeState {
            open: true,
            mode: Some("reply".to_string()),
            account_id: Some(account_id.clone()),
            to: seed
                .recipients
                .into_iter()
                .map(|(value, _)| value)
                .collect(),
            subject: Some(reply_subject(selected_subject(state))),
            body: String::new(),
            in_reply_to: selected_in_reply_to(state),
            quoted_text: selected_quote_text(state),
            quoted_from: Some(selected_sender(state)),
            signature_loading: true,
            revision: state.compose.revision.saturating_add(1),
            ..ComposeState::default()
        };
        state.selected_view = SelectedView::Mail;
        state.last_error = None;
        cx.notify();
        Some((
            state.api_base_url.clone(),
            account_id,
            state.compose.revision,
        ))
    });

    if let Some((base_url, account_id, revision)) = load {
        start_compose_signature_load(state_entity, base_url, account_id, revision, app_cx);
    }
}

pub(crate) fn start_forward_draft(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    let load = state_entity.update(app_cx, |state, cx| {
        if state.selected_message.is_none() {
            let message = "Select a message before forwarding.".to_string();
            state.compose.error = Some(message.clone());
            state.last_error = Some(message);
            cx.notify();
            return None;
        }
        let Some(account_id) = selected_account_id(state) else {
            let message = "No account is available for this message.".to_string();
            state.compose.error = Some(message.clone());
            state.last_error = Some(message);
            cx.notify();
            return None;
        };

        state.compose = ComposeState {
            open: true,
            mode: Some("forward".to_string()),
            account_id: Some(account_id.clone()),
            subject: Some(forward_subject(selected_subject(state))),
            body: selected_forward_body(state),
            quoted_text: selected_quote_text(state),
            quoted_from: Some(selected_sender(state)),
            signature_loading: true,
            revision: state.compose.revision.saturating_add(1),
            ..ComposeState::default()
        };
        state.selected_view = SelectedView::Mail;
        state.last_error = None;
        cx.notify();
        Some((
            state.api_base_url.clone(),
            account_id,
            state.compose.revision,
        ))
    });

    if let Some((base_url, account_id, revision)) = load {
        start_compose_signature_load(state_entity, base_url, account_id, revision, app_cx);
    }
}

pub(crate) fn start_compose_signature_load(
    state_entity: &Entity<NativeAppState>,
    base_url: String,
    account_id: String,
    revision: u64,
    app_cx: &mut App,
) {
    let task = app_cx.background_executor().spawn(async move {
        load_account_signature(base_url, account_id.clone())
            .map(|signature| (account_id, signature))
    });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                state_entity.update(&mut async_cx, |state, cx| {
                    if state.compose.revision != revision || !state.compose.open {
                        return;
                    }
                    state.compose.signature_loading = false;
                    match result {
                        Ok((account_id, settings)) => {
                            if state.compose.account_id.as_deref() == Some(account_id.as_str()) {
                                state.compose.signature = settings
                                    .signature
                                    .filter(|signature| !signature.trim().is_empty());
                            }
                        }
                        Err(err) => {
                            tracing::warn!("failed to load native compose signature: {err:#}");
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();
}

pub(crate) fn load_account_signature(
    base_url: String,
    account_id: String,
) -> Result<AccountSettings> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .account_settings(&account_id)
            .await
            .with_context(|| format!("get account settings for {account_id}"))
    })
}

pub(crate) fn start_recipient_suggestions_load(
    state_entity: &Entity<NativeAppState>,
    query: String,
    app_cx: &mut App,
) {
    let query = query.trim().to_string();
    let Some((base_url, request)) = state_entity.update(app_cx, |state, cx| {
        if query.len() < 2 {
            clear_recipient_suggestions(&mut state.compose);
            cx.notify();
            return None;
        }

        if state.compose.recipient_suggestions_query.as_deref() == Some(query.as_str())
            && (state.compose.recipient_suggestions_resource.is_loading()
                || state.compose.recipient_suggestions_resource.is_ready())
        {
            return None;
        }

        state.compose.recipient_suggestions_query = Some(query.clone());
        state.compose.recipient_suggestions.clear();
        let request = state.compose.recipient_suggestions_resource.begin_load();
        cx.notify();
        Some((state.api_base_url.clone(), request))
    }) else {
        return;
    };

    let task_query = query.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { load_recipient_suggestions(base_url, task_query) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(contacts) => {
                            apply_loaded_recipient_suggestions(state, request, &query, contacts);
                        }
                        Err(err) => {
                            reject_recipient_suggestions_load(state, request, &query, err);
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();
}

pub(crate) fn load_recipient_suggestions(base_url: String, query: String) -> Result<Vec<Contact>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .suggest_contacts(&query)
            .await
            .with_context(|| format!("suggest contacts for {query}"))
    })
}

pub(crate) fn apply_loaded_recipient_suggestions(
    state: &mut NativeAppState,
    request: ResourceRequest,
    query: &str,
    contacts: Vec<Contact>,
) -> bool {
    if state.compose.recipient_suggestions_query.as_deref() != Some(query) {
        return false;
    }
    if !state
        .compose
        .recipient_suggestions_resource
        .resolve(request, contacts.clone())
    {
        return false;
    }

    state.compose.recipient_suggestions = contacts;
    true
}

pub(crate) fn reject_recipient_suggestions_load(
    state: &mut NativeAppState,
    request: ResourceRequest,
    query: &str,
    err: anyhow::Error,
) -> bool {
    if state.compose.recipient_suggestions_query.as_deref() != Some(query) {
        return false;
    }

    let message = err.to_string();
    if !state
        .compose
        .recipient_suggestions_resource
        .reject(request, message)
    {
        return false;
    }

    state.compose.recipient_suggestions.clear();
    true
}

pub(crate) fn clear_recipient_suggestions(compose: &mut ComposeState) {
    compose.recipient_suggestions_query = None;
    compose.recipient_suggestions.clear();
    compose.recipient_suggestions_resource.reset();
}

pub(crate) fn update_compose_recipients(
    state_entity: &Entity<NativeAppState>,
    field: ComposeRecipientField,
    tokens: Vec<TokenInputToken>,
    app_cx: &mut App,
) {
    let values = tokens
        .into_iter()
        .filter(|token| token.valid)
        .map(|token| token.value.to_string())
        .collect::<Vec<_>>();
    state_entity.update(app_cx, |state, cx| {
        match field {
            ComposeRecipientField::To => state.compose.to = values,
            ComposeRecipientField::Cc => state.compose.cc = values,
            ComposeRecipientField::Bcc => state.compose.bcc = values,
        }
        cx.notify();
    });
}

pub(crate) fn update_compose_subject(
    state_entity: &Entity<NativeAppState>,
    subject: String,
    app_cx: &mut App,
) {
    state_entity.update(app_cx, |state, cx| {
        state.compose.subject = Some(subject);
        cx.notify();
    });
}

pub(crate) fn update_compose_body(
    state_entity: &Entity<NativeAppState>,
    body: String,
    app_cx: &mut App,
) {
    state_entity.update(app_cx, |state, cx| {
        state.compose.body = body;
        cx.notify();
    });
}

pub(crate) fn compose_tokens(values: &[String]) -> Vec<TokenInputToken> {
    values
        .iter()
        .map(|value| TokenInputToken::new(value.clone(), value.clone()))
        .collect()
}

pub(crate) fn start_snooze_from_picker(
    state_entity: &Entity<NativeAppState>,
    snooze_picker: Entity<DateTimePicker>,
    message_id: String,
    app_cx: &mut App,
) {
    let value = snooze_picker
        .read(app_cx)
        .selected_value()
        .unwrap_or_else(default_snooze_value);
    start_snooze_with_value(state_entity, message_id, value, app_cx);
}

pub(crate) fn start_snooze_with_preset(
    state_entity: &Entity<NativeAppState>,
    preset: SnoozePreset,
    message_id: String,
    app_cx: &mut App,
) {
    let value = snooze_preset_value_at(preset, Local::now());
    start_snooze_with_value(state_entity, message_id, value, app_cx);
}

pub(crate) fn start_snooze_with_value(
    state_entity: &Entity<NativeAppState>,
    message_id: String,
    value: DateTimeValue,
    app_cx: &mut App,
) {
    let until = match date_time_value_to_utc_iso(value) {
        Ok(until) => until,
        Err(err) => {
            state_entity.update(app_cx, |state, cx| {
                state.last_error = Some(err.to_string());
                cx.notify();
            });
            return;
        }
    };

    start_message_mutation_and_refresh(
        state_entity,
        NativeMessageMutation::Snooze { message_id, until },
        app_cx,
    );
}

pub(crate) fn start_quick_reply_send(
    state_entity: &Entity<NativeAppState>,
    recipient_input: Entity<TokenInput>,
    quick_reply_input: Entity<TextInput>,
    app_cx: &mut App,
) {
    let recipients = recipient_input
        .read(app_cx)
        .values()
        .iter()
        .filter(|token| token.valid)
        .map(|token| token.value.to_string())
        .collect::<Vec<_>>();
    let body = quick_reply_input.read(app_cx).text().trim().to_string();

    let Some((base_url, request)) = state_entity.update(app_cx, |state, cx| {
        match build_quick_reply_request(state, recipients, body) {
            Ok(request) => {
                state.compose.sending = true;
                state.compose.error = None;
                state.compose.notice = None;
                state.last_error = None;
                cx.notify();
                Some((state.api_base_url.clone(), request))
            }
            Err(message) => {
                state.compose.error = Some(message.clone());
                state.last_error = Some(message);
                cx.notify();
                None
            }
        }
    }) else {
        return;
    };

    let task = app_cx
        .background_executor()
        .spawn(async move { send_quick_reply(base_url, request) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let sent = result.is_ok();
                let error = result.err().map(|err| err.to_string());

                state_entity.update(&mut async_cx, |state, cx| {
                    state.compose.sending = false;
                    if sent {
                        state.compose.open = false;
                        state.compose.error = None;
                        state.compose.notice = Some("Reply sent".to_string());
                        state.last_error = None;
                    } else if let Some(error) = error.clone() {
                        state.compose.error = Some(error.clone());
                        state.last_error = Some(error);
                    }
                    cx.notify();
                });

                if sent {
                    quick_reply_input.update(&mut async_cx, |input, cx| {
                        input.set_value("", cx);
                    });
                    recipient_input.update(&mut async_cx, |input, cx| {
                        input.set_tokens(Vec::<TokenInputToken>::new(), cx);
                    });
                }
            }
        })
        .detach();
}

pub(crate) fn start_compose_send(
    state_entity: &Entity<NativeAppState>,
    to_input: Entity<TokenInput>,
    cc_input: Entity<TokenInput>,
    bcc_input: Entity<TokenInput>,
    subject_input: Entity<TextInput>,
    body_input: Entity<Textarea>,
    app_cx: &mut App,
) {
    let to = token_input_values(&to_input, app_cx);
    let cc = token_input_values(&cc_input, app_cx);
    let bcc = token_input_values(&bcc_input, app_cx);
    let subject = subject_input.read(app_cx).text().to_string();
    let body = body_input.read(app_cx).text().to_string();

    let Some((base_url, request)) = state_entity.update(app_cx, |state, cx| {
        match build_compose_request(state, to, cc, bcc, subject, body) {
            Ok(request) => {
                state.compose.sending = true;
                state.compose.error = None;
                state.compose.notice = None;
                state.last_error = None;
                cx.notify();
                Some((state.api_base_url.clone(), request))
            }
            Err(message) => {
                state.compose.error = Some(message.clone());
                state.last_error = Some(message);
                cx.notify();
                None
            }
        }
    }) else {
        return;
    };

    let task = app_cx
        .background_executor()
        .spawn(async move { send_message_request(base_url, request) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let sent = result.is_ok();
                let error = result.err().map(|err| err.to_string());

                state_entity.update(&mut async_cx, |state, cx| {
                    state.compose.sending = false;
                    if sent {
                        reset_compose_state(state, Some("Message sent".to_string()));
                        state.last_error = None;
                    } else if let Some(error) = error.clone() {
                        state.compose.error = Some(error.clone());
                        state.last_error = Some(error);
                    }
                    cx.notify();
                });

                if sent {
                    to_input.update(&mut async_cx, |input, cx| {
                        input.set_tokens(Vec::<TokenInputToken>::new(), cx);
                    });
                    cc_input.update(&mut async_cx, |input, cx| {
                        input.set_tokens(Vec::<TokenInputToken>::new(), cx);
                    });
                    bcc_input.update(&mut async_cx, |input, cx| {
                        input.set_tokens(Vec::<TokenInputToken>::new(), cx);
                    });
                    subject_input.update(&mut async_cx, |input, cx| {
                        input.set_value("", cx);
                    });
                    body_input.update(&mut async_cx, |input, cx| {
                        input.set_value("", cx);
                    });
                }
            }
        })
        .detach();
}

pub(crate) fn discard_compose(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        reset_compose_state(state, None);
        state.last_error = None;
        cx.notify();
    });
}

pub(crate) fn token_input_values(input: &Entity<TokenInput>, app_cx: &App) -> Vec<String> {
    input
        .read(app_cx)
        .values()
        .iter()
        .filter(|token| token.valid)
        .map(|token| token.value.to_string())
        .collect()
}

pub(crate) fn reset_compose_state(state: &mut NativeAppState, notice: Option<String>) {
    let revision = state.compose.revision.saturating_add(1);
    state.compose = ComposeState {
        revision,
        notice,
        ..ComposeState::default()
    };
}

pub(crate) fn send_quick_reply(base_url: String, request: SendRequest) -> Result<()> {
    send_message_request(base_url, request)
}

pub(crate) fn send_message_request(base_url: String, request: SendRequest) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client.send_message(&request).await?;
        Ok(())
    })
}

pub(crate) fn build_quick_reply_request(
    state: &NativeAppState,
    recipients: Vec<String>,
    body: String,
) -> std::result::Result<SendRequest, String> {
    let body = body.trim().to_string();
    if body.is_empty() {
        return Err("Write a message before sending.".to_string());
    }

    let mode = state.compose.mode.as_deref().unwrap_or("reply");
    let mut to = normalize_recipients(recipients);
    if to.is_empty() && mode != "forward" {
        if let Some(email) = selected_from_email(state) {
            to.push(email);
        }
    }
    if to.is_empty() {
        return Err("Add at least one recipient.".to_string());
    }

    let account_id = state
        .compose
        .account_id
        .clone()
        .or_else(|| selected_account_id(state))
        .ok_or_else(|| "No account is available for this message.".to_string())?;
    let subject = state.compose.subject.clone().unwrap_or_else(|| {
        if mode == "forward" {
            forward_subject(selected_subject(state))
        } else {
            reply_subject(selected_subject(state))
        }
    });
    let in_reply_to = if mode == "forward" {
        None
    } else {
        state
            .compose
            .in_reply_to
            .clone()
            .or_else(|| selected_in_reply_to(state))
    };

    Ok(SendRequest {
        account_id,
        to,
        cc: Vec::new(),
        bcc: Vec::new(),
        subject,
        text_body: Some(body),
        html_body: None,
        in_reply_to,
        attachments: Vec::new(),
        send_at: None,
        send_delay_seconds: None,
    })
}

pub(crate) fn build_compose_request(
    state: &NativeAppState,
    to: Vec<String>,
    cc: Vec<String>,
    bcc: Vec<String>,
    subject: String,
    body: String,
) -> std::result::Result<SendRequest, String> {
    let mode = state.compose.mode.as_deref().unwrap_or("compose");
    if state.compose.signature_loading {
        return Err("Wait for the account signature to load.".to_string());
    }

    let mut to = normalize_recipients(to);
    if to.is_empty() && mode == "reply" {
        if let Some(email) = selected_from_email(state) {
            to.push(email);
        }
    }
    if to.is_empty() {
        return Err("Add at least one recipient.".to_string());
    }

    let text_body = build_compose_text_body(state, mode, body)?;
    let account_id = state
        .compose
        .account_id
        .clone()
        .or_else(|| selected_account_id(state))
        .or_else(|| state.accounts.first().map(|account| account.id.clone()))
        .ok_or_else(|| "No account is available for this message.".to_string())?;
    let subject = subject.trim().to_string();
    let in_reply_to = if mode == "reply" {
        state
            .compose
            .in_reply_to
            .clone()
            .or_else(|| selected_in_reply_to(state))
    } else {
        None
    };

    Ok(SendRequest {
        account_id,
        to,
        cc: normalize_recipients(cc),
        bcc: normalize_recipients(bcc),
        subject,
        text_body: Some(text_body),
        html_body: None,
        in_reply_to,
        attachments: Vec::new(),
        send_at: None,
        send_delay_seconds: None,
    })
}

pub(crate) fn build_compose_text_body(
    state: &NativeAppState,
    mode: &str,
    body: String,
) -> std::result::Result<String, String> {
    let mut text_body = body.trim().to_string();
    if text_body.is_empty() {
        return Err("Write a message before sending.".to_string());
    }

    if mode == "reply" {
        if let Some(quote) = state
            .compose
            .quoted_text
            .as_deref()
            .map(str::trim)
            .filter(|quote| !quote.is_empty())
        {
            text_body.push_str("\n\n");
            text_body.push_str(&build_quoted_block(
                quote,
                state.compose.quoted_from.as_deref(),
            ));
        }
    }

    if let Some(signature) = state
        .compose
        .signature
        .as_deref()
        .map(str::trim)
        .filter(|signature| !signature.is_empty())
    {
        text_body.push_str("\n\n-- \n");
        text_body.push_str(signature);
    }

    Ok(text_body)
}

pub(crate) fn build_quoted_block(text: &str, from: Option<&str>) -> String {
    let prefix = from
        .map(str::trim)
        .filter(|from| !from.is_empty())
        .map(|from| format!("{from} wrote:\n"))
        .unwrap_or_else(|| "Original message:\n".to_string());
    format!(
        "{prefix}{}",
        text.lines()
            .map(|line| format!("> {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

pub(crate) fn build_forward_body(text: &str, from: Option<&str>) -> String {
    let header = from
        .map(str::trim)
        .filter(|from| !from.is_empty())
        .map(|from| format!("---------- Forwarded message from {from} ----------\n\n"))
        .unwrap_or_else(|| "---------- Forwarded message ----------\n\n".to_string());
    format!("{header}{}", text.trim())
}

pub(crate) fn selected_reply_seed(state: &NativeAppState) -> Option<NativeDraftSeed> {
    let email = selected_from_email(state)?;
    let label = selected_sender(state);
    Some(NativeDraftSeed {
        recipients: vec![(email, label)],
    })
}

pub(crate) fn selected_account_id(state: &NativeAppState) -> Option<String> {
    state
        .selected_full_message
        .as_ref()
        .map(|message| message.account_id.clone())
        .or_else(|| selected_summary(state).map(|message| message.account_id.clone()))
}

pub(crate) fn selected_in_reply_to(state: &NativeAppState) -> Option<String> {
    state
        .selected_full_message
        .as_ref()
        .and_then(|message| message.message_id.clone())
        .or_else(|| selected_summary(state).and_then(|message| message.message_id.clone()))
}

pub(crate) fn selected_from_email(state: &NativeAppState) -> Option<String> {
    state
        .selected_full_message
        .as_ref()
        .and_then(|message| message.from_email.clone())
        .or_else(|| selected_summary(state).and_then(|message| message.from_email.clone()))
        .map(|email| email.trim().to_string())
        .filter(|email| !email.is_empty())
}

pub(crate) fn selected_sender_email(state: &NativeAppState) -> Option<String> {
    selected_from_email(state)
}

pub(crate) fn selected_forward_body(state: &NativeAppState) -> String {
    let body = selected_quote_text(state).unwrap_or_default();

    if body.is_empty() {
        return String::new();
    }

    build_forward_body(&body, Some(&selected_sender(state)))
}

pub(crate) fn selected_quote_text(state: &NativeAppState) -> Option<String> {
    state
        .selected_full_message
        .as_ref()
        .and_then(|message| message.body.as_ref())
        .and_then(|body| body.text_body.as_ref())
        .or_else(|| selected_summary(state).and_then(|message| message.preview.as_ref()))
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

pub(crate) fn reply_subject(subject: String) -> String {
    if subject.trim_start().to_ascii_lowercase().starts_with("re:") {
        subject
    } else {
        format!("Re: {subject}")
    }
}

pub(crate) fn forward_subject(subject: String) -> String {
    if subject
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("fwd:")
    {
        subject
    } else {
        format!("Fwd: {subject}")
    }
}

pub(crate) fn normalize_recipients(recipients: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    recipients
        .into_iter()
        .map(|recipient| recipient.trim().to_string())
        .filter(|recipient| !recipient.is_empty())
        .filter(|recipient| seen.insert(recipient.to_ascii_lowercase()))
        .collect()
}

pub(crate) fn date_time_value_to_utc_iso(value: DateTimeValue) -> Result<String> {
    let local = Local
        .with_ymd_and_hms(
            value.year,
            value.month.into(),
            value.day.into(),
            value.hour.into(),
            value.minute.into(),
            0,
        )
        .single()
        .ok_or_else(|| anyhow!("invalid local snooze time {}", value.display_value()))?;
    Ok(local.with_timezone(&Utc).to_rfc3339())
}

pub(crate) fn recipient_suggestions(state: &NativeAppState) -> Vec<TokenInputSuggestion> {
    let mut seen = BTreeSet::new();
    let mut suggestions = Vec::new();

    for contact in &state.compose.recipient_suggestions {
        push_recipient_suggestion(
            &mut suggestions,
            &mut seen,
            contact.email.as_str(),
            contact.name.as_deref(),
        );
    }

    for message in &state.messages {
        let Some(email) = message.from_email.as_deref().map(str::trim) else {
            continue;
        };
        push_recipient_suggestion(
            &mut suggestions,
            &mut seen,
            email,
            message.from_name.as_deref(),
        );
    }

    suggestions
}

pub(crate) fn push_recipient_suggestion(
    suggestions: &mut Vec<TokenInputSuggestion>,
    seen: &mut BTreeSet<String>,
    email: &str,
    name: Option<&str>,
) {
    let email = email.trim();
    if email.is_empty() || !seen.insert(email.to_lowercase()) {
        return;
    }

    let label = name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(email)
        .to_string();

    let mut suggestion = TokenInputSuggestion::new(email.to_string(), label.clone());
    if label != email {
        suggestion = suggestion.detail(email.to_string());
    }
    suggestions.push(suggestion);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_and_forward_subjects_are_not_double_prefixed() {
        assert_eq!(reply_subject("Hello".into()), "Re: Hello");
        assert_eq!(reply_subject("  RE: Hello".into()), "  RE: Hello");
        assert_eq!(forward_subject("Hello".into()), "Fwd: Hello");
        assert_eq!(forward_subject("fwd: Hello".into()), "fwd: Hello");
    }
}
