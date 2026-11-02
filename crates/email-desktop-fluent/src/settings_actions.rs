use crate::prelude::*;
use crate::*;

pub(crate) fn open_settings_modal(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    app_cx: &mut App,
) {
    state_entity.update(app_cx, |state, cx| {
        state.modals.settings_open = true;
        state.modals.advanced_search_open = false;
        cx.notify();
    });
    modal_entity.update(app_cx, |modal, cx| {
        modal.reset(cx);
    });
}

pub(crate) fn close_settings_modal(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        state.modals.settings_open = false;
        cx.notify();
    });
}

pub(crate) fn handle_settings_modal_key(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    key: &str,
    command: bool,
    app_cx: &mut App,
) -> bool {
    match key {
        "escape" => {
            let editor_open = modal_entity.read(app_cx).rule_editor_mode.is_some();
            if editor_open {
                cancel_settings_rule_editor(modal_entity, app_cx);
            } else {
                close_settings_modal(state_entity, app_cx);
            }
            true
        }
        "enter" | "return" => {
            let active = {
                let modal = modal_entity.read(app_cx);
                (modal.active_tab.clone(), modal.rule_editor_mode.is_some())
            };
            match active {
                (SettingsTab::Rules(_), true) => {
                    start_settings_rule_save(state_entity, modal_entity, app_cx);
                    true
                }
                (SettingsTab::Webhooks, _) => {
                    start_settings_webhook_create(state_entity, modal_entity, app_cx);
                    true
                }
                (SettingsTab::Account(account_id), _) if command => {
                    start_settings_save(state_entity, modal_entity, account_id, app_cx);
                    true
                }
                _ => false,
            }
        }
        _ => false,
    }
}

pub(crate) fn select_settings_account(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    app_cx: &mut App,
) {
    let selection = {
        let state = state_entity.read(app_cx);
        state
            .accounts
            .iter()
            .find(|account| account.id == account_id)
            .cloned()
            .map(|account| (state.api_base_url.clone(), account))
    };

    let Some((base_url, account)) = selection else {
        let message = "Account not found.".to_string();
        state_entity.update(app_cx, |state, cx| {
            state.last_error = Some(message.clone());
            cx.notify();
        });
        modal_entity.update(app_cx, |modal, cx| {
            modal.set_status(Some((message, false)), cx);
        });
        return;
    };

    let account_id = account.id.clone();
    modal_entity.update(app_cx, |modal, cx| {
        modal.select_account(&account, cx);
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { load_account_signature(base_url, account_id.clone()) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    let requested_account_id = account.id.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| match result {
                    Ok(settings) => modal.apply_account_settings(settings, cx),
                    Err(err) => {
                        modal.fail_account_settings(&requested_account_id, err.to_string(), cx);
                    }
                }) {
                    tracing::warn!("failed to apply native account settings load: {err}");
                }

                if let Some(message) = error_message {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(message);
                        cx.notify();
                    }) {
                        tracing::warn!("failed to apply native account settings error: {err}");
                    }
                }
            }
        })
        .detach();
}

pub(crate) fn select_settings_webhooks(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    app_cx: &mut App,
) {
    let base_url = state_entity.read(app_cx).api_base_url.clone();
    modal_entity.update(app_cx, |modal, cx| {
        modal.select_webhooks(cx);
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { load_settings_webhooks(base_url) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| match result {
                    Ok(webhooks) => modal.apply_webhooks(webhooks, None, false, cx),
                    Err(err) => modal.fail_webhooks(err.to_string(), cx),
                }) {
                    tracing::warn!("failed to apply native webhooks load: {err}");
                }

                if let Some(message) = error_message {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(message);
                        cx.notify();
                    }) {
                        tracing::warn!("failed to apply native webhooks error: {err}");
                    }
                }
            }
        })
        .detach();
}

pub(crate) fn select_settings_rules(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    app_cx: &mut App,
) {
    let base_url = state_entity.read(app_cx).api_base_url.clone();
    modal_entity.update(app_cx, |modal, cx| {
        modal.select_rules(account_id.clone(), cx);
    });

    let requested_account_id = account_id.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { load_settings_rules(base_url, account_id.clone()) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| match result {
                    Ok(rules) => modal.apply_rules(&requested_account_id, rules, None, cx),
                    Err(err) => {
                        modal.fail_rules(&requested_account_id, err.to_string(), cx);
                    }
                }) {
                    tracing::warn!("failed to apply native rules load: {err}");
                }

                if let Some(message) = error_message {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(message);
                        cx.notify();
                    }) {
                        tracing::warn!("failed to apply native rules error: {err}");
                    }
                }
            }
        })
        .detach();
}

pub(crate) fn confirm_settings_account_delete(
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    app_cx: &mut App,
) {
    modal_entity.update(app_cx, |modal, cx| {
        modal.confirm_delete_account = Some(account_id);
        modal.status = None;
        cx.notify();
    });
}

pub(crate) fn cancel_settings_account_delete(
    modal_entity: &Entity<SettingsModal>,
    app_cx: &mut App,
) {
    modal_entity.update(app_cx, |modal, cx| {
        modal.confirm_delete_account = None;
        modal.status = None;
        cx.notify();
    });
}

pub(crate) fn start_settings_save(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    app_cx: &mut App,
) {
    let (name, sync_days, signature) = {
        let modal = modal_entity.read(app_cx);
        (
            modal.name_input.read(app_cx).text().to_string(),
            modal.sync_days_input.read(app_cx).text().to_string(),
            modal.signature_input.read(app_cx).text().to_string(),
        )
    };

    let request = match build_account_settings_update_request(name, sync_days, signature) {
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

    let (base_url, refresh_request) = state_entity.update(app_cx, |state, cx| {
        let refresh_request = begin_sync_complete_refresh(state);
        state.last_error = None;
        cx.notify();
        (state.api_base_url.clone(), refresh_request)
    });
    modal_entity.update(app_cx, |modal, cx| {
        modal.saving_account = true;
        modal.status = None;
        cx.notify();
    });

    let load_request = refresh_request.clone();
    let requested_account_id = account_id.clone();
    let task = app_cx.background_executor().spawn(async move {
        update_account_settings_and_refresh(base_url, account_id.clone(), request, load_request)
    });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let modal_status = match &result {
                    Ok(_) => ("Saved".to_string(), true),
                    Err(err) => (err.to_string(), false),
                };

                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_loaded_sync_refresh(state, refresh_request, loaded);
                        }
                        Err(err) => {
                            reject_sync_refresh(state, refresh_request, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native settings save refresh: {err}");
                }

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| {
                    if modal.active_tab == SettingsTab::Account(requested_account_id.clone()) {
                        modal.set_status(Some(modal_status), cx);
                    } else {
                        modal.saving_account = false;
                        cx.notify();
                    }
                }) {
                    tracing::warn!("failed to apply native settings save status: {err}");
                }
            }
        })
        .detach();
}

pub(crate) fn start_settings_account_delete(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    app_cx: &mut App,
) {
    let Some(base_url) = ({
        let state = state_entity.read(app_cx);
        state
            .accounts
            .iter()
            .any(|account| account.id == account_id)
            .then(|| state.api_base_url.clone())
    }) else {
        let message = "Account not found.".to_string();
        modal_entity.update(app_cx, |modal, cx| {
            modal.set_status(Some((message.clone(), false)), cx);
        });
        state_entity.update(app_cx, |state, cx| {
            state.last_error = Some(message);
            cx.notify();
        });
        return;
    };

    modal_entity.update(app_cx, |modal, cx| {
        modal.deleting_account = true;
        modal.status = None;
        cx.notify();
    });
    state_entity.update(app_cx, |state, cx| {
        state.last_error = None;
        cx.notify();
    });

    let requested_account_id = account_id.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { delete_account_and_reload(base_url, account_id) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_account_delete_reload(state, &requested_account_id, loaded);
                        }
                        Err(err) => {
                            state.last_error = Some(err.to_string());
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native account delete reload: {err}");
                }

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| {
                    if let Some(message) = error_message {
                        modal.set_status(Some((message, false)), cx);
                    } else {
                        modal.reset(cx);
                    }
                }) {
                    tracing::warn!("failed to apply native account delete status: {err}");
                }
            }
        })
        .detach();
}

pub(crate) fn start_settings_rule_toggle(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    rule_id: String,
    app_cx: &mut App,
) {
    start_settings_rule_mutation(
        state_entity,
        modal_entity,
        account_id,
        SettingsRuleMutation::Toggle {
            rule_id: rule_id.clone(),
        },
        rule_id,
        "Rule updated",
        app_cx,
    );
}

pub(crate) fn start_settings_rule_delete(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    rule_id: String,
    app_cx: &mut App,
) {
    start_settings_rule_mutation(
        state_entity,
        modal_entity,
        account_id,
        SettingsRuleMutation::Delete {
            rule_id: rule_id.clone(),
        },
        rule_id,
        "Rule deleted",
        app_cx,
    );
}

pub(crate) fn start_settings_rule_new(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    app_cx: &mut App,
) {
    let folders = state_entity
        .read(app_cx)
        .folders_by_account
        .get(&account_id)
        .cloned()
        .unwrap_or_default();
    modal_entity.update(app_cx, |modal, cx| {
        modal.start_new_rule_editor(account_id, folders, cx);
    });
}

pub(crate) fn start_settings_rule_edit(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    rule: RuleDto,
    app_cx: &mut App,
) {
    let folders = state_entity
        .read(app_cx)
        .folders_by_account
        .get(&account_id)
        .cloned()
        .unwrap_or_default();
    modal_entity.update(app_cx, |modal, cx| {
        modal.start_edit_rule_editor(account_id, rule, folders, cx);
    });
}

pub(crate) fn cancel_settings_rule_editor(modal_entity: &Entity<SettingsModal>, app_cx: &mut App) {
    modal_entity.update(app_cx, |modal, cx| {
        modal.reset_rule_editor(cx);
        modal.status = None;
        cx.notify();
    });
}

pub(crate) fn start_settings_rule_save(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    app_cx: &mut App,
) {
    let (saving, mode, account_id, name_input, match_mode, conditions, actions) = {
        let modal = modal_entity.read(app_cx);
        (
            modal.rule_saving,
            modal.rule_editor_mode.clone(),
            modal.rule_editor_account_id.clone(),
            modal.rule_name_input.clone(),
            modal.rule_match_mode.clone(),
            modal
                .rule_condition_rows
                .iter()
                .map(|row| row.condition.clone())
                .collect::<Vec<_>>(),
            modal
                .rule_action_rows
                .iter()
                .map(|row| SettingsRuleActionDraft {
                    action_type: row.action_type.clone(),
                    action_value: row.action_value.clone(),
                })
                .collect::<Vec<_>>(),
        )
    };
    if saving {
        return;
    }
    let name = name_input.read(app_cx).text().to_string();
    let request = match build_settings_rule_save_request(
        mode, account_id, name, match_mode, conditions, actions,
    ) {
        Ok(request) => request,
        Err(message) => {
            modal_entity.update(app_cx, |modal, cx| {
                modal.status = Some((message.clone(), false));
                modal.rule_saving = false;
                cx.notify();
            });
            state_entity.update(app_cx, |state, cx| {
                state.last_error = Some(message);
                cx.notify();
            });
            return;
        }
    };

    let base_url = state_entity.read(app_cx).api_base_url.clone();
    let account_id = request.account_id.clone();
    modal_entity.update(app_cx, |modal, cx| {
        modal.rule_saving = true;
        modal.status = None;
        cx.notify();
    });
    state_entity.update(app_cx, |state, cx| {
        state.last_error = None;
        cx.notify();
    });

    let requested_account_id = account_id.clone();
    let task = app_cx.background_executor().spawn(async move {
        mutate_settings_rules(base_url, account_id, SettingsRuleMutation::Save(request))
    });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| match result {
                    Ok(rules) => {
                        modal.apply_rules(
                            &requested_account_id,
                            rules,
                            Some(("Rule saved".to_string(), true)),
                            cx,
                        );
                        modal.reset_rule_editor(cx);
                    }
                    Err(err) => {
                        modal.fail_rules(&requested_account_id, err.to_string(), cx);
                    }
                }) {
                    tracing::warn!("failed to apply native rule save: {err}");
                }

                if let Some(message) = error_message {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(message);
                        cx.notify();
                    }) {
                        tracing::warn!("failed to apply native rule save error: {err}");
                    }
                }
            }
        })
        .detach();
}

pub(crate) fn start_settings_rule_mutation(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    mutation: SettingsRuleMutation,
    busy_id: String,
    success_message: &'static str,
    app_cx: &mut App,
) {
    let base_url = state_entity.read(app_cx).api_base_url.clone();
    modal_entity.update(app_cx, |modal, cx| {
        modal.rule_busy_ids.insert(busy_id);
        modal.status = None;
        cx.notify();
    });
    state_entity.update(app_cx, |state, cx| {
        state.last_error = None;
        cx.notify();
    });

    let requested_account_id = account_id.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { mutate_settings_rules(base_url, account_id, mutation) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| match result {
                    Ok(rules) => modal.apply_rules(
                        &requested_account_id,
                        rules,
                        Some((success_message.to_string(), true)),
                        cx,
                    ),
                    Err(err) => {
                        modal.fail_rules(&requested_account_id, err.to_string(), cx);
                    }
                }) {
                    tracing::warn!("failed to apply native rule mutation: {err}");
                }

                if let Some(message) = error_message {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(message);
                        cx.notify();
                    }) {
                        tracing::warn!("failed to apply native rule mutation error: {err}");
                    }
                }
            }
        })
        .detach();
}

pub(crate) fn start_settings_webhook_create(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    app_cx: &mut App,
) {
    let (url, secret, events) = {
        let modal = modal_entity.read(app_cx);
        (
            modal.webhook_url_input.read(app_cx).text().to_string(),
            modal.webhook_secret_input.read(app_cx).text().to_string(),
            modal.webhook_events.clone(),
        )
    };
    let request = match build_create_webhook_request(url, secret, events) {
        Ok(request) => request,
        Err(message) => {
            modal_entity.update(app_cx, |modal, cx| {
                modal.fail_webhooks(message.clone(), cx);
            });
            state_entity.update(app_cx, |state, cx| {
                state.last_error = Some(message);
                cx.notify();
            });
            return;
        }
    };

    start_settings_webhook_mutation(
        state_entity,
        modal_entity,
        SettingsWebhookMutation::Create(request),
        None,
        "Webhook added",
        true,
        app_cx,
    );
}

pub(crate) fn start_settings_webhook_toggle(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    webhook_id: String,
    enabled: bool,
    app_cx: &mut App,
) {
    start_settings_webhook_mutation(
        state_entity,
        modal_entity,
        SettingsWebhookMutation::Toggle {
            webhook_id: webhook_id.clone(),
            enabled,
        },
        Some(webhook_id),
        "Webhook updated",
        false,
        app_cx,
    );
}

pub(crate) fn start_settings_webhook_delete(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    webhook_id: String,
    app_cx: &mut App,
) {
    start_settings_webhook_mutation(
        state_entity,
        modal_entity,
        SettingsWebhookMutation::Delete {
            webhook_id: webhook_id.clone(),
        },
        Some(webhook_id),
        "Webhook deleted",
        false,
        app_cx,
    );
}

pub(crate) fn start_settings_webhook_mutation(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    mutation: SettingsWebhookMutation,
    busy_id: Option<String>,
    success_message: &'static str,
    clear_new: bool,
    app_cx: &mut App,
) {
    let base_url = state_entity.read(app_cx).api_base_url.clone();
    modal_entity.update(app_cx, |modal, cx| {
        if let Some(id) = busy_id.as_ref() {
            modal.webhook_busy_ids.insert(id.clone());
        } else {
            modal.adding_webhook = true;
        }
        modal.status = None;
        cx.notify();
    });
    state_entity.update(app_cx, |state, cx| {
        state.last_error = None;
        cx.notify();
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { mutate_settings_webhooks(base_url, mutation) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| match result {
                    Ok(webhooks) => modal.apply_webhooks(
                        webhooks,
                        Some((success_message.to_string(), true)),
                        clear_new,
                        cx,
                    ),
                    Err(err) => modal.fail_webhooks(err.to_string(), cx),
                }) {
                    tracing::warn!("failed to apply native webhook mutation: {err}");
                }

                if let Some(message) = error_message {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(message);
                        cx.notify();
                    }) {
                        tracing::warn!("failed to apply native webhook mutation error: {err}");
                    }
                }
            }
        })
        .detach();
}

pub(crate) fn start_folder_exclusion_toggle(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    folder_id: String,
    next_excluded: bool,
    app_cx: &mut App,
) {
    let (base_url, refresh_request) = state_entity.update(app_cx, |state, cx| {
        let refresh_request = begin_sync_complete_refresh(state);
        state.last_error = None;
        cx.notify();
        (state.api_base_url.clone(), refresh_request)
    });
    modal_entity.update(app_cx, |modal, cx| {
        modal.status = Some(("Updating folder...".to_string(), true));
        cx.notify();
    });

    let load_request = refresh_request.clone();
    let task = app_cx.background_executor().spawn(async move {
        patch_folder_exclusion_and_refresh(base_url, folder_id, next_excluded, load_request)
    });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let modal_status = match &result {
                    Ok(_) => ("Folder updated".to_string(), true),
                    Err(err) => (err.to_string(), false),
                };

                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_loaded_sync_refresh(state, refresh_request, loaded);
                        }
                        Err(err) => {
                            reject_sync_refresh(state, refresh_request, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native folder exclusion refresh: {err}");
                }

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| {
                    modal.set_status(Some(modal_status), cx);
                }) {
                    tracing::warn!("failed to apply native folder exclusion status: {err}");
                }
            }
        })
        .detach();
}

pub(crate) fn load_settings_webhooks(base_url: String) -> Result<Vec<Webhook>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client.list_webhooks().await.context("list webhooks")
    })
}

pub(crate) fn load_settings_rules(base_url: String, account_id: String) -> Result<Vec<RuleDto>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .list_rules(&account_id)
            .await
            .with_context(|| format!("list rules for {account_id}"))
    })
}

pub(crate) fn mutate_settings_rules(
    base_url: String,
    account_id: String,
    mutation: SettingsRuleMutation,
) -> Result<Vec<RuleDto>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        match mutation {
            SettingsRuleMutation::Save(request) => match request.mode {
                SettingsRuleEditorMode::New => {
                    client
                        .create_rule(
                            &request.account_id,
                            &request.name,
                            Some(&request.match_mode),
                            Some(request.conditions),
                            Some(request.actions),
                        )
                        .await
                        .with_context(|| format!("create rule for {}", request.account_id))?;
                }
                SettingsRuleEditorMode::Edit(rule_id) => {
                    client
                        .update_rule(
                            &rule_id,
                            &UpdateRuleBody {
                                name: Some(request.name),
                                match_mode: Some(request.match_mode),
                                conditions: Some(request.conditions),
                                actions: Some(request.actions),
                            },
                        )
                        .await
                        .with_context(|| format!("update rule {rule_id}"))?;
                }
            },
            SettingsRuleMutation::Toggle { rule_id } => {
                client
                    .toggle_rule(&rule_id)
                    .await
                    .with_context(|| format!("toggle rule {rule_id}"))?;
            }
            SettingsRuleMutation::Delete { rule_id } => {
                client
                    .delete_rule(&rule_id)
                    .await
                    .with_context(|| format!("delete rule {rule_id}"))?;
            }
        }
        client
            .list_rules(&account_id)
            .await
            .with_context(|| format!("reload rules for {account_id}"))
    })
}

pub(crate) fn mutate_settings_webhooks(
    base_url: String,
    mutation: SettingsWebhookMutation,
) -> Result<Vec<Webhook>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        match mutation {
            SettingsWebhookMutation::Create(request) => {
                client
                    .create_webhook(&request)
                    .await
                    .context("create webhook")?;
            }
            SettingsWebhookMutation::Toggle {
                webhook_id,
                enabled,
            } => {
                client
                    .update_webhook(
                        &webhook_id,
                        &UpdateWebhook {
                            url: None,
                            secret: None,
                            events: None,
                            enabled: Some(enabled),
                        },
                    )
                    .await
                    .with_context(|| format!("update webhook {webhook_id}"))?;
            }
            SettingsWebhookMutation::Delete { webhook_id } => {
                client
                    .delete_webhook(&webhook_id)
                    .await
                    .with_context(|| format!("delete webhook {webhook_id}"))?;
            }
        }
        client.list_webhooks().await.context("reload webhooks")
    })
}

pub(crate) fn build_create_webhook_request(
    url: String,
    secret: String,
    events: String,
) -> std::result::Result<CreateWebhook, String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("Enter a webhook URL.".to_string());
    }

    let events = match events.trim() {
        "" | "new_message" => "new_message".to_string(),
        "all" => "all".to_string(),
        _ => return Err("Choose a supported webhook event set.".to_string()),
    };
    let secret = if secret.trim().is_empty() {
        None
    } else {
        Some(secret.trim().to_string())
    };

    Ok(CreateWebhook {
        url,
        secret,
        events: Some(events),
        account_id: None,
    })
}

pub(crate) fn build_settings_rule_save_request(
    mode: Option<SettingsRuleEditorMode>,
    account_id: Option<String>,
    name: String,
    match_mode: MatchMode,
    conditions: Vec<Condition>,
    actions: Vec<SettingsRuleActionDraft>,
) -> std::result::Result<SettingsRuleSaveRequest, String> {
    let mode = mode.ok_or_else(|| "Open a rule editor first.".to_string())?;
    let account_id = account_id
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "Rule account not found.".to_string())?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Enter a rule name.".to_string());
    }
    if actions.is_empty() {
        return Err("Add at least one rule action.".to_string());
    }

    let conditions = conditions
        .into_iter()
        .map(|condition| {
            let value = if condition_field_type(&condition.field) == ConditionFieldType::Boolean {
                if condition.value.trim() == "false" {
                    "false".to_string()
                } else {
                    "true".to_string()
                }
            } else {
                condition.value.trim().to_string()
            };
            ConditionInput {
                field: condition_field_value(&condition.field).to_string(),
                operator: condition_operator_value(&condition.operator).to_string(),
                value,
            }
        })
        .collect::<Vec<_>>();

    let mut request_actions = Vec::with_capacity(actions.len());
    for action in actions {
        let action_type = action.action_type.trim().to_string();
        if !is_supported_rule_action(&action_type) {
            return Err("Choose a supported rule action.".to_string());
        }

        let action_value = if rule_action_needs_folder(&action_type) {
            let value = action.action_value.unwrap_or_default().trim().to_string();
            if value.is_empty() {
                return Err("Choose a folder for move actions.".to_string());
            }
            Some(value)
        } else {
            None
        };

        request_actions.push(ActionInput {
            action_type,
            action_value,
        });
    }

    Ok(SettingsRuleSaveRequest {
        mode,
        account_id,
        name,
        match_mode: match_mode_value(&match_mode).to_string(),
        conditions,
        actions: request_actions,
    })
}

pub(crate) fn match_mode_value(match_mode: &MatchMode) -> &'static str {
    match match_mode {
        MatchMode::All => "all",
        MatchMode::Any => "any",
    }
}

pub(crate) fn is_supported_rule_action(value: &str) -> bool {
    rule_action_label(value).is_some()
}

pub(crate) fn rule_action_needs_folder(value: &str) -> bool {
    value == "move_to_folder"
}

pub(crate) fn rule_action_label(value: &str) -> Option<&'static str> {
    match value {
        "mark_read" => Some("Mark as read"),
        "mark_unread" => Some("Mark as unread"),
        "flag" => Some("Flag message"),
        "unflag" => Some("Unflag message"),
        "archive" => Some("Archive"),
        "delete" => Some("Delete"),
        "move_to_folder" => Some("Move to folder"),
        _ => None,
    }
}

pub(crate) fn rule_folder_option_label(folder: &Folder) -> String {
    if folder.full_path.trim().is_empty() {
        folder.name.clone()
    } else {
        folder.full_path.clone()
    }
}

pub(crate) fn rule_counts_text(rule: &RuleDto) -> String {
    format!(
        "{} condition{}, {} action{}",
        rule.conditions.len(),
        if rule.conditions.len() == 1 { "" } else { "s" },
        rule.actions.len(),
        if rule.actions.len() == 1 { "" } else { "s" }
    )
}

pub(crate) fn update_account_settings_and_refresh(
    base_url: String,
    account_id: String,
    request: UpdateAccount,
    refresh_request: NativeSyncRefreshRequest,
) -> Result<LoadedSyncRefresh> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .update_account(&account_id, &request)
            .await
            .with_context(|| format!("update account settings for {account_id}"))?;
        load_sync_refresh_with_client(&client, refresh_request).await
    })
}

pub(crate) fn patch_folder_exclusion_and_refresh(
    base_url: String,
    folder_id: String,
    next_excluded: bool,
    refresh_request: NativeSyncRefreshRequest,
) -> Result<LoadedSyncRefresh> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .patch_folder(
                &folder_id,
                &PatchFolder {
                    is_excluded: Some(next_excluded),
                },
            )
            .await
            .with_context(|| format!("update folder exclusion for {folder_id}"))?;
        load_sync_refresh_with_client(&client, refresh_request).await
    })
}

pub(crate) async fn load_sync_refresh_with_client(
    client: &EmailClient,
    request: NativeSyncRefreshRequest,
) -> Result<LoadedSyncRefresh> {
    let tree = load_account_tree(client).await?;
    let messages = if let Some(source) = request.source {
        Some(load_message_collection_with_client(client, source, request.selected_message).await?)
    } else {
        None
    };

    Ok(LoadedSyncRefresh {
        accounts: tree.accounts,
        folders_by_account: tree.folders_by_account,
        labels: tree.labels,
        messages,
    })
}

pub(crate) fn build_account_settings_update_request(
    name: String,
    sync_days: String,
    signature: String,
) -> std::result::Result<UpdateAccount, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Enter a display name.".to_string());
    }

    let sync_days_limit = if sync_days.trim().is_empty() {
        None
    } else {
        let parsed = sync_days
            .trim()
            .parse::<i64>()
            .map_err(|_| "Download days must be a positive whole number.".to_string())?;
        if parsed < 1 {
            return Err("Download days must be a positive whole number.".to_string());
        }
        Some(parsed)
    };

    let signature = if signature.trim().is_empty() {
        None
    } else {
        Some(signature)
    };

    Ok(UpdateAccount {
        name: Some(name),
        sync_days_limit: Some(sync_days_limit),
        signature: Some(signature),
    })
}
