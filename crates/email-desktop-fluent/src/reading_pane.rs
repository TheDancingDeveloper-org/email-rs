use crate::prelude::*;
use crate::*;

#[derive(Clone)]
pub(crate) struct ReadingPanePlaceholder {
    pub(crate) state: Entity<NativeAppState>,
    pub(crate) context_menu: Option<Entity<ContextMenu>>,
    pub(crate) recipient_input: Entity<TokenInput>,
    pub(crate) quick_reply_input: Entity<TextInput>,
    pub(crate) compose_to_input: Entity<TokenInput>,
    pub(crate) compose_cc_input: Entity<TokenInput>,
    pub(crate) compose_bcc_input: Entity<TokenInput>,
    pub(crate) compose_subject_input: Entity<TextInput>,
    pub(crate) compose_body_input: Entity<Textarea>,
    pub(crate) snooze_picker: Entity<DateTimePicker>,
    pub(crate) synced_compose_revision: u64,
    pub(crate) synced_recipient_suggestions: Vec<TokenInputSuggestion>,
}

impl ReadingPanePlaceholder {
    pub(crate) fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let suggestions = recipient_suggestions(state.read(cx));
        let compose_suggestions = suggestions.clone();
        let recipient_state = state.clone();
        let recipient_input = cx.new(move |_| {
            TokenInput::new()
                .placeholder("Add recipient")
                .suggestions(suggestions.clone())
                .allow_freeform(true)
                .on_query_change({
                    let state = recipient_state.clone();
                    move |query, app_cx| {
                        start_recipient_suggestions_load(&state, query.to_string(), app_cx);
                    }
                })
        });
        let quick_reply_input = cx.new(|_| TextInput::new().placeholder("Write a quick reply"));
        let compose_to_input = cx.new({
            let state = state.clone();
            let suggestions = compose_suggestions.clone();
            move |_| {
                TokenInput::new()
                    .placeholder("recipient@example.com")
                    .suggestions(suggestions.clone())
                    .allow_freeform(true)
                    .on_query_change({
                        let state = state.clone();
                        move |query, app_cx| {
                            start_recipient_suggestions_load(&state, query.to_string(), app_cx);
                        }
                    })
                    .on_change({
                        let state = state.clone();
                        move |tokens, app_cx| {
                            update_compose_recipients(
                                &state,
                                ComposeRecipientField::To,
                                tokens,
                                app_cx,
                            );
                        }
                    })
            }
        });
        let compose_cc_input = cx.new({
            let state = state.clone();
            let suggestions = compose_suggestions.clone();
            move |_| {
                TokenInput::new()
                    .placeholder("cc@example.com")
                    .suggestions(suggestions.clone())
                    .allow_freeform(true)
                    .on_query_change({
                        let state = state.clone();
                        move |query, app_cx| {
                            start_recipient_suggestions_load(&state, query.to_string(), app_cx);
                        }
                    })
                    .on_change({
                        let state = state.clone();
                        move |tokens, app_cx| {
                            update_compose_recipients(
                                &state,
                                ComposeRecipientField::Cc,
                                tokens,
                                app_cx,
                            );
                        }
                    })
            }
        });
        let compose_bcc_input = cx.new({
            let state = state.clone();
            let suggestions = compose_suggestions.clone();
            move |_| {
                TokenInput::new()
                    .placeholder("bcc@example.com")
                    .suggestions(suggestions.clone())
                    .allow_freeform(true)
                    .on_query_change({
                        let state = state.clone();
                        move |query, app_cx| {
                            start_recipient_suggestions_load(&state, query.to_string(), app_cx);
                        }
                    })
                    .on_change({
                        let state = state.clone();
                        move |tokens, app_cx| {
                            update_compose_recipients(
                                &state,
                                ComposeRecipientField::Bcc,
                                tokens,
                                app_cx,
                            );
                        }
                    })
            }
        });
        let compose_subject_input = cx.new({
            let state = state.clone();
            move |_| {
                TextInput::new().placeholder("Subject").on_change({
                    let state = state.clone();
                    move |subject, app_cx| {
                        update_compose_subject(&state, subject.to_string(), app_cx);
                    }
                })
            }
        });
        let compose_body_input = cx.new({
            let state = state.clone();
            move |_| {
                Textarea::new()
                    .placeholder("Write your message...")
                    .rows(12)
                    .on_change({
                        let state = state.clone();
                        move |body, app_cx| {
                            update_compose_body(&state, body.to_string(), app_cx);
                        }
                    })
            }
        });
        let default_snooze = default_snooze_value();
        let snooze_picker = cx.new(move |_| {
            DateTimePicker::new()
                .placeholder("Select local time")
                .default_value(default_snooze)
        });
        Self {
            state,
            context_menu: None,
            recipient_input,
            quick_reply_input,
            compose_to_input,
            compose_cc_input,
            compose_bcc_input,
            compose_subject_input,
            compose_body_input,
            snooze_picker,
            synced_compose_revision: 0,
            synced_recipient_suggestions: compose_suggestions,
        }
    }
}

impl Render for ReadingPanePlaceholder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        self.sync_recipient_suggestions(&state, cx);
        self.sync_compose_inputs(&state, cx);
        if state.compose.open {
            return compose_placeholder(
                &self.state,
                &state,
                self.compose_to_input.clone(),
                self.compose_cc_input.clone(),
                self.compose_bcc_input.clone(),
                self.compose_subject_input.clone(),
                self.compose_body_input.clone(),
                cx,
            );
        }

        let mut pane = reading_placeholder(
            &self.state,
            &state,
            self.recipient_input.clone(),
            self.quick_reply_input.clone(),
            self.snooze_picker.clone(),
            cx,
        );

        if let Some(menu_entity) = &self.context_menu {
            if menu_entity.read(cx).open {
                pane = pane.child(menu_entity.clone());
            }
        }

        pane
    }
}

impl ReadingPanePlaceholder {
    pub(crate) fn sync_recipient_suggestions(
        &mut self,
        state: &NativeAppState,
        cx: &mut Context<Self>,
    ) {
        let suggestions = recipient_suggestions(state);
        if suggestions == self.synced_recipient_suggestions {
            return;
        }

        for input in [
            self.recipient_input.clone(),
            self.compose_to_input.clone(),
            self.compose_cc_input.clone(),
            self.compose_bcc_input.clone(),
        ] {
            let suggestions = suggestions.clone();
            input.update(cx, |input, cx| {
                input.set_suggestions(suggestions, cx);
            });
        }
        self.synced_recipient_suggestions = suggestions;
    }

    pub(crate) fn sync_compose_inputs(&mut self, state: &NativeAppState, cx: &mut Context<Self>) {
        if state.compose.revision == self.synced_compose_revision {
            return;
        }

        // Use set_tokens_silent so programmatic resets don't fire on_change
        // callbacks back into app state (avoids re-entrant state updates during
        // render and fixes spurious Discard / Bcc no-op appearances).
        self.compose_to_input.update(cx, |input, cx| {
            input.set_tokens_silent(compose_tokens(&state.compose.to), cx);
        });
        self.compose_cc_input.update(cx, |input, cx| {
            input.set_tokens_silent(compose_tokens(&state.compose.cc), cx);
        });
        self.compose_bcc_input.update(cx, |input, cx| {
            input.set_tokens_silent(compose_tokens(&state.compose.bcc), cx);
        });
        self.compose_subject_input.update(cx, |input, cx| {
            input.set_value(state.compose.subject.clone().unwrap_or_default(), cx);
        });
        self.compose_body_input.update(cx, |input, cx| {
            input.set_value(state.compose.body.clone(), cx);
        });
        self.synced_compose_revision = state.compose.revision;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadingPaneKeyAction {
    Archive,
    Delete,
    ToggleRead,
    ClearSelection,
    CopyBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SnoozePreset {
    LaterToday,
    TomorrowMorning,
    NextMonday,
}

pub(crate) fn reading_pane_key_action(
    key: &str,
    shift: bool,
    command: bool,
) -> Option<ReadingPaneKeyAction> {
    if key == "escape" {
        return Some(ReadingPaneKeyAction::ClearSelection);
    }
    if command {
        return match key {
            "c" => Some(ReadingPaneKeyAction::CopyBody),
            _ => None,
        };
    }

    match key {
        "e" => Some(ReadingPaneKeyAction::Archive),
        "#" => Some(ReadingPaneKeyAction::Delete),
        "3" if shift => Some(ReadingPaneKeyAction::Delete),
        "delete" | "backspace" => Some(ReadingPaneKeyAction::Delete),
        "u" => Some(ReadingPaneKeyAction::ToggleRead),
        _ => None,
    }
}

pub(crate) fn handle_reading_pane_key(
    state_entity: &Entity<NativeAppState>,
    key: &str,
    shift: bool,
    command: bool,
    app_cx: &mut App,
) -> bool {
    let Some(action) = reading_pane_key_action(key, shift, command) else {
        return false;
    };

    if action == ReadingPaneKeyAction::ClearSelection {
        clear_reading_selection(state_entity, app_cx);
        return true;
    }

    if action == ReadingPaneKeyAction::CopyBody {
        let text = {
            let state = state_entity.read(app_cx);
            state
                .selected_full_message
                .as_ref()
                .and_then(|m| m.body.as_ref())
                .and_then(|b| {
                    b.text_body.clone().or_else(|| {
                        b.html_body
                            .as_ref()
                            .map(|h| RichDocument::from_html_fragment_lossy(h).to_plain_text())
                    })
                })
                .unwrap_or_default()
        };
        if !text.is_empty() {
            app_cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        return true;
    }

    let (message_id, is_read, busy) = {
        let state = state_entity.read(app_cx);
        (
            state.selected_message.clone(),
            state
                .selected_full_message
                .as_ref()
                .map(|message| message.is_read)
                .or_else(|| selected_summary(state).map(|message| message.is_read))
                .unwrap_or(true),
            state.messages_resource.is_loading() || state.compose.sending,
        )
    };
    if busy {
        return true;
    }

    let Some(message_id) = message_id else {
        return false;
    };
    match action {
        ReadingPaneKeyAction::Archive => start_message_mutation_and_refresh(
            state_entity,
            NativeMessageMutation::Archive { message_id },
            app_cx,
        ),
        ReadingPaneKeyAction::Delete => start_message_mutation_and_refresh(
            state_entity,
            NativeMessageMutation::Delete { message_id },
            app_cx,
        ),
        ReadingPaneKeyAction::ToggleRead => start_message_mutation_and_refresh(
            state_entity,
            NativeMessageMutation::SetRead {
                message_id,
                is_read: !is_read,
            },
            app_cx,
        ),
        ReadingPaneKeyAction::ClearSelection => {}
        // CopyBody is fully handled above before we fetch message_id.
        ReadingPaneKeyAction::CopyBody => {}
    }
    true
}

pub(crate) fn clear_reading_selection(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        state.selected_message = None;
        state.selected_full_message = None;
        state.selected_message_resource.reset();
        state.selected_message_labels.clear();
        state.selected_message_labels_resource.reset();
        reset_compose_state(state, None);
        cx.notify();
    });
}

pub(crate) fn copy_selected_sender_email(
    state_entity: &Entity<NativeAppState>,
    app_cx: &mut App,
) -> bool {
    let email = selected_sender_email(state_entity.read(app_cx));
    let Some(email) = email else {
        let message = "No sender email to copy.".to_string();
        state_entity.update(app_cx, |state, cx| {
            state.compose.error = Some(message.clone());
            state.compose.notice = None;
            state.last_error = Some(message);
            cx.notify();
        });
        return false;
    };

    app_cx.write_to_clipboard(ClipboardItem::new_string(email));
    state_entity.update(app_cx, |state, cx| {
        state.compose.error = None;
        state.compose.notice = Some("Sender email copied".to_string());
        state.last_error = None;
        cx.notify();
    });
    true
}

pub(crate) fn reading_header_context_menu(
    state_entity: Entity<NativeAppState>,
    message_id: String,
    is_read: bool,
    position: Point<Pixels>,
    app_cx: &mut App,
) -> Entity<ContextMenu> {
    let reply_state = state_entity.clone();
    let forward_state = state_entity.clone();
    let archive_state = state_entity.clone();
    let delete_state = state_entity.clone();
    let read_state = state_entity.clone();
    let copy_state = state_entity;

    app_cx.new(move |_| {
        ContextMenu::build(position)
            .action("Reply", move |_, _, app_cx| {
                start_reply_draft(&reply_state, app_cx);
            })
            .action("Forward", move |_, _, app_cx| {
                start_forward_draft(&forward_state, app_cx);
            })
            .separator()
            .action("Archive", {
                let message_id = message_id.clone();
                move |_, _, app_cx| {
                    start_message_mutation_and_refresh(
                        &archive_state,
                        NativeMessageMutation::Archive {
                            message_id: message_id.clone(),
                        },
                        app_cx,
                    );
                }
            })
            .action("Delete", {
                let message_id = message_id.clone();
                move |_, _, app_cx| {
                    start_message_mutation_and_refresh(
                        &delete_state,
                        NativeMessageMutation::Delete {
                            message_id: message_id.clone(),
                        },
                        app_cx,
                    );
                }
            })
            .action(if is_read { "Mark unread" } else { "Mark read" }, {
                let message_id = message_id.clone();
                move |_, _, app_cx| {
                    start_message_mutation_and_refresh(
                        &read_state,
                        NativeMessageMutation::SetRead {
                            message_id: message_id.clone(),
                            is_read: !is_read,
                        },
                        app_cx,
                    );
                }
            })
            .separator()
            .action("Copy sender email", move |_, _, app_cx| {
                copy_selected_sender_email(&copy_state, app_cx);
            })
    })
}

pub(crate) fn selected_summary(state: &NativeAppState) -> Option<&Message> {
    let selected_id = state.selected_message.as_deref()?;
    state
        .messages
        .iter()
        .find(|message| message.id == selected_id)
}

pub(crate) fn selected_subject(state: &NativeAppState) -> String {
    state
        .selected_full_message
        .as_ref()
        .and_then(|message| message.subject.clone())
        .or_else(|| selected_summary(state).and_then(|message| message.subject.clone()))
        .filter(|subject| !subject.trim().is_empty())
        .unwrap_or_else(|| "(no subject)".to_string())
}

pub(crate) fn selected_sender(state: &NativeAppState) -> String {
    state
        .selected_full_message
        .as_ref()
        .and_then(|message| {
            message
                .from_name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .or_else(|| message.from_email.clone())
        })
        .or_else(|| {
            selected_summary(state).and_then(|message| {
                message
                    .from_name
                    .clone()
                    .filter(|name| !name.trim().is_empty())
                    .or_else(|| message.from_email.clone())
            })
        })
        .unwrap_or_else(|| "Unknown sender".to_string())
}

pub(crate) fn selected_date(state: &NativeAppState) -> Option<String> {
    state
        .selected_full_message
        .as_ref()
        .and_then(|message| message.date.clone())
        .or_else(|| selected_summary(state).and_then(|message| message.date.clone()))
        .filter(|date| !date.trim().is_empty())
}

pub(crate) fn reading_badge(
    label: &'static str,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    div()
        .h(px(24.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .rounded(px(999.0))
        .bg(theme.colors.neutral)
        .text_size(px(theme.typography.caption.size))
        .text_color(theme.colors.on_subtle)
        .child(label)
}

pub(crate) fn reading_attachment_section(
    state_entity: &Entity<NativeAppState>,
    message_id: &str,
    attachments: &[MessageAttachment],
    busy: bool,
    notice: Option<&str>,
    error: Option<&str>,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let status = error
        .map(|message| (message, true))
        .or_else(|| notice.map(|message| (message, false)));
    let mut section = div()
        .border_t_1()
        .border_color(colors.stroke_neutral_subtle)
        .px(px(20.0))
        .py(px(12.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(typography.caption.size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.on_subtle)
                .child("Attachments"),
        );

    if attachments.is_empty() {
        section = section.child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("Attachment metadata is not available for this message."),
        );
    } else {
        for attachment in attachments {
            section = section.child(reading_attachment_row(
                state_entity,
                message_id,
                attachment,
                busy,
                cx,
            ));
        }
    }

    if let Some((message, is_error)) = status {
        section = section.child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(if is_error {
                    colors.status_error
                } else {
                    colors.on_subtle
                })
                .child(message.to_string()),
        );
    }

    section
}

pub(crate) fn reading_attachment_row(
    state_entity: &Entity<NativeAppState>,
    message_id: &str,
    attachment: &MessageAttachment,
    busy: bool,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let meta = format!(
        "{} - {}",
        attachment.content_type,
        attachment_size_label(attachment.size_bytes)
    );
    let open_attachment = attachment.clone();
    let download_attachment = attachment.clone();
    let open_message_id = message_id.to_string();
    let download_message_id = message_id.to_string();

    div()
        .id(SharedString::from(format!(
            "reading-attachment-row-{}",
            attachment.id
        )))
        .px(px(9.0))
        .py(px(8.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(colors.stroke_neutral_subtle)
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            svg()
                .path("icons/download.svg")
                .size(px(15.0))
                .text_color(colors.on_subtle),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(typography.body.size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.on_neutral)
                        .child(attachment.filename.clone()),
                )
                .child(
                    div()
                        .pt(px(2.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(typography.caption.size))
                        .text_color(colors.on_subtle)
                        .child(meta),
                ),
        )
        .child(
            Button::new(SharedString::from(format!(
                "reading-attachment-open-{}",
                attachment.id
            )))
            .label(if busy { "Opening..." } else { "Open" })
            .appearance(ButtonAppearance::Neutral)
            .size(ButtonSize::Compact)
            .disabled(busy)
            .on_click({
                let state_entity = state_entity.clone();
                move |_, _, app_cx| {
                    start_attachment_open(
                        &state_entity,
                        &open_message_id,
                        open_attachment.clone(),
                        app_cx,
                    );
                }
            }),
        )
        .child(
            Button::new(SharedString::from(format!(
                "reading-attachment-download-{}",
                attachment.id
            )))
            .label(if busy { "Saving..." } else { "Download" })
            .appearance(ButtonAppearance::Subtle)
            .size(ButtonSize::Compact)
            .disabled(busy)
            .on_click({
                let state_entity = state_entity.clone();
                move |_, _, app_cx| {
                    start_attachment_download(
                        &state_entity,
                        &download_message_id,
                        download_attachment.clone(),
                        app_cx,
                    );
                }
            }),
        )
        .into_any_element()
}

pub(crate) fn reading_snooze_controls(
    state_entity: &Entity<NativeAppState>,
    message_id: &str,
    action_busy: bool,
    snooze_picker: Entity<DateTimePicker>,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;

    div()
        .pt(px(8.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_wrap()
        .child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(colors.on_subtle)
                .child("Snooze"),
        )
        .child(
            Button::new("reading-snooze-later-today")
                .label("Later today")
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .disabled(action_busy)
                .on_click({
                    let state_entity = state_entity.clone();
                    let message_id = message_id.to_string();
                    move |_, _, app_cx| {
                        start_snooze_with_preset(
                            &state_entity,
                            SnoozePreset::LaterToday,
                            message_id.clone(),
                            app_cx,
                        );
                    }
                }),
        )
        .child(
            Button::new("reading-snooze-tomorrow")
                .label("Tomorrow")
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .disabled(action_busy)
                .on_click({
                    let state_entity = state_entity.clone();
                    let message_id = message_id.to_string();
                    move |_, _, app_cx| {
                        start_snooze_with_preset(
                            &state_entity,
                            SnoozePreset::TomorrowMorning,
                            message_id.clone(),
                            app_cx,
                        );
                    }
                }),
        )
        .child(
            Button::new("reading-snooze-next-week")
                .label("Next week")
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .disabled(action_busy)
                .on_click({
                    let state_entity = state_entity.clone();
                    let message_id = message_id.to_string();
                    move |_, _, app_cx| {
                        start_snooze_with_preset(
                            &state_entity,
                            SnoozePreset::NextMonday,
                            message_id.clone(),
                            app_cx,
                        );
                    }
                }),
        )
        .child(div().w(px(190.0)).child(snooze_picker.clone()))
        .child(
            Button::new("reading-snooze-custom")
                .label("Custom")
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .disabled(action_busy)
                .on_click({
                    let state_entity = state_entity.clone();
                    let message_id = message_id.to_string();
                    move |_, _, app_cx| {
                        start_snooze_from_picker(
                            &state_entity,
                            snooze_picker.clone(),
                            message_id.clone(),
                            app_cx,
                        );
                    }
                }),
        )
}

pub(crate) fn reading_label_row(
    state_entity: &Entity<NativeAppState>,
    state: &NativeAppState,
    message_id: &str,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let selected_account_id = selected_account_id(state);
    let attached_ids = state
        .selected_message_labels
        .iter()
        .map(|label| label.id.as_str())
        .collect::<BTreeSet<_>>();
    let addable_labels = state
        .labels
        .iter()
        .filter(|label| {
            selected_account_id
                .as_deref()
                .is_none_or(|id| label.account_id == id)
        })
        .filter(|label| !attached_ids.contains(label.id.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    let mut row = div()
        .pt(px(10.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_wrap();

    if state.selected_message_labels.is_empty()
        && state.selected_message_labels_resource.is_loading()
    {
        row = row.child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(colors.on_subtle)
                .child("Loading labels..."),
        );
    }

    for (idx, label) in state.selected_message_labels.iter().enumerate() {
        let state_entity = state_entity.clone();
        let message_id = message_id.to_string();
        let label_id = label.id.clone();
        row = row.child(
            Button::new(("reading-remove-label", idx))
                .label(format!("{} x", label.name))
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .disabled(state.selected_message_labels_resource.is_loading())
                .on_click(move |_, _, app_cx| {
                    start_message_label_mutation(
                        &state_entity,
                        NativeLabelMutation::Remove {
                            message_id: message_id.clone(),
                            label_id: label_id.clone(),
                        },
                        app_cx,
                    );
                }),
        );
    }

    for (idx, label) in addable_labels.into_iter().enumerate() {
        let state_entity = state_entity.clone();
        let message_id = message_id.to_string();
        let label_id = label.id.clone();
        row = row.child(
            Button::new(("reading-add-label", idx))
                .label(format!("+ {}", label.name))
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .disabled(state.selected_message_labels_resource.is_loading())
                .on_click(move |_, _, app_cx| {
                    start_message_label_mutation(
                        &state_entity,
                        NativeLabelMutation::Add {
                            message_id: message_id.clone(),
                            label_id: label_id.clone(),
                        },
                        app_cx,
                    );
                }),
        );
    }

    if let Some(error) = state.selected_message_labels_resource.error() {
        row = row.child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(colors.status_error)
                .child(format!("Labels failed: {error}")),
        );
    }

    row
}

pub(crate) fn reading_placeholder(
    state_entity: &Entity<NativeAppState>,
    state: &NativeAppState,
    recipient_input: Entity<TokenInput>,
    quick_reply_input: Entity<TextInput>,
    snooze_picker: Entity<DateTimePicker>,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let selected_id = state.selected_message.clone();
    let body_status = if state.selected_message_resource.is_loading() {
        Some("Loading message body...".to_string())
    } else {
        state
            .selected_message_resource
            .error()
            .map(|error| format!("Message body failed to load: {error}"))
    };

    if selected_id.is_none() {
        let empty_text = if initial_accounts_loading(state) {
            "Loading accounts..."
        } else {
            "Select a message to read it."
        };
        return div()
            .size_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(colors.stroke_neutral_subtle)
            .bg(colors.surface)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w(px(320.0))
                            .text_size(px(typography.body.size))
                            .text_color(colors.on_subtle)
                            .child(empty_text),
                    ),
            );
    }

    let subject = selected_subject(state);
    let sender = selected_sender(state);
    let date = selected_date(state).unwrap_or_else(|| "No date".to_string());
    let summary = selected_summary(state);
    let is_flagged = state
        .selected_full_message
        .as_ref()
        .map(|message| message.is_flagged)
        .or_else(|| summary.map(|message| message.is_flagged))
        .unwrap_or(false);
    let has_attachments = state
        .selected_full_message
        .as_ref()
        .map(|message| message.has_attachments || !message.attachments.is_empty())
        .or_else(|| summary.map(|message| message.has_attachments))
        .unwrap_or(false);
    let attachments = state
        .selected_full_message
        .as_ref()
        .map(|message| message.attachments.clone())
        .unwrap_or_default();
    let is_read = state
        .selected_full_message
        .as_ref()
        .map(|message| message.is_read)
        .or_else(|| summary.map(|message| message.is_read))
        .unwrap_or(true);
    let message_id = selected_id.unwrap_or_default();
    let action_busy = state.messages_resource.is_loading() || state.compose.sending;
    let quick_reply_has_body = !quick_reply_input.read(cx).text().trim().is_empty();
    let quick_reply_has_recipient =
        !recipient_input.read(cx).values().is_empty() || selected_from_email(state).is_some();
    let can_send_quick_reply =
        !state.compose.sending && quick_reply_has_body && quick_reply_has_recipient;
    let compose_status = state
        .compose
        .error
        .clone()
        .map(|error| (error, true))
        .or_else(|| state.compose.notice.clone().map(|notice| (notice, false)));
    let pane_entity = cx.entity();

    div()
        .tab_index(0)
        .key_context("NativeReadingPane")
        .on_key_down({
            let state_entity = state_entity.clone();
            let recipient_input = recipient_input.clone();
            let quick_reply_input = quick_reply_input.clone();
            let snooze_picker = snooze_picker.clone();
            move |ev: &KeyDownEvent, window, app_cx| {
                let editing = recipient_input.focus_handle(app_cx).is_focused(window)
                    || quick_reply_input.focus_handle(app_cx).is_focused(window)
                    || snooze_picker.focus_handle(app_cx).is_focused(window);
                if editing {
                    return;
                }

                if handle_reading_pane_key(
                    &state_entity,
                    ev.keystroke.key.as_str(),
                    ev.keystroke.modifiers.shift,
                    ev.keystroke.modifiers.control || ev.keystroke.modifiers.platform,
                    app_cx,
                ) {
                    app_cx.stop_propagation();
                }
            }
        })
        .size_full()
        .flex()
        .flex_col()
        .border_l_1()
        .border_color(colors.stroke_neutral_subtle)
        .bg(colors.surface)
        .child(
            div()
                .px(px(20.0))
                .py(px(14.0))
                .border_b_1()
                .border_color(colors.stroke_neutral_subtle)
                .on_mouse_down(MouseButton::Right, {
                    let state_entity = state_entity.clone();
                    let pane_entity = pane_entity.clone();
                    let message_id = message_id.clone();
                    move |ev, _, app_cx| {
                        let menu = reading_header_context_menu(
                            state_entity.clone(),
                            message_id.clone(),
                            is_read,
                            ev.position,
                            app_cx,
                        );
                        pane_entity.update(app_cx, |pane, cx| {
                            pane.context_menu = Some(menu);
                            cx.notify();
                        });
                        app_cx.stop_propagation();
                    }
                })
                .child(
                    div()
                        .text_size(px(typography.subtitle.size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.on_neutral)
                        .child(subject),
                )
                .child(
                    div()
                        .pt(px(5.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(typography.body.size))
                                .text_color(colors.on_subtle)
                                .child(sender),
                        )
                        .child(
                            div()
                                .text_size(px(typography.caption.size))
                                .text_color(colors.on_subtle)
                                .child(date),
                        ),
                )
                .child(reading_label_row(state_entity, state, &message_id, cx))
                .child(
                    div()
                        .pt(px(10.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            Button::new("reading-reply")
                                .label("Reply")
                                .appearance(ButtonAppearance::Neutral)
                                .size(ButtonSize::Compact)
                                .disabled(action_busy)
                                .on_click({
                                    let state_entity = state_entity.clone();
                                    move |_, _, app_cx| {
                                        start_reply_draft(&state_entity, app_cx);
                                    }
                                }),
                        )
                        .child(
                            Button::new("reading-forward")
                                .label("Forward")
                                .appearance(ButtonAppearance::Neutral)
                                .size(ButtonSize::Compact)
                                .disabled(action_busy)
                                .on_click({
                                    let state_entity = state_entity.clone();
                                    move |_, _, app_cx| {
                                        start_forward_draft(&state_entity, app_cx);
                                    }
                                }),
                        )
                        // (e) Copy body text to clipboard
                        .child({
                            let state_entity = state_entity.clone();
                            Button::new("reading-copy-body")
                                .label("Copy")
                                .appearance(ButtonAppearance::Subtle)
                                .size(ButtonSize::Compact)
                                .on_click(move |_, _, app_cx| {
                                    let text = state_entity
                                        .read(app_cx)
                                        .selected_full_message
                                        .as_ref()
                                        .and_then(|m| m.body.as_ref())
                                        .and_then(|b| {
                                            b.text_body.clone().or_else(|| {
                                                b.html_body.as_ref().map(|h| {
                                                    RichDocument::from_html_fragment_lossy(h)
                                                        .to_plain_text()
                                                })
                                            })
                                        })
                                        .unwrap_or_default();
                                    if !text.is_empty() {
                                        app_cx.write_to_clipboard(ClipboardItem::new_string(text));
                                    }
                                })
                        })
                        .child(
                            Button::new("reading-toggle-read")
                                .label(if is_read { "Mark unread" } else { "Mark read" })
                                .appearance(ButtonAppearance::Subtle)
                                .size(ButtonSize::Compact)
                                .disabled(action_busy)
                                .on_click({
                                    let state_entity = state_entity.clone();
                                    let message_id = message_id.clone();
                                    move |_, _, app_cx| {
                                        start_message_mutation_and_refresh(
                                            &state_entity,
                                            NativeMessageMutation::SetRead {
                                                message_id: message_id.clone(),
                                                is_read: !is_read,
                                            },
                                            app_cx,
                                        );
                                    }
                                }),
                        )
                        .child(
                            Button::new("reading-archive")
                                .label("Archive")
                                .appearance(ButtonAppearance::Subtle)
                                .size(ButtonSize::Compact)
                                .disabled(action_busy)
                                .on_click({
                                    let state_entity = state_entity.clone();
                                    let message_id = message_id.clone();
                                    move |_, _, app_cx| {
                                        start_message_mutation_and_refresh(
                                            &state_entity,
                                            NativeMessageMutation::Archive {
                                                message_id: message_id.clone(),
                                            },
                                            app_cx,
                                        );
                                    }
                                }),
                        )
                        .child(
                            Button::new("reading-delete")
                                .label("Delete")
                                .appearance(ButtonAppearance::Danger)
                                .size(ButtonSize::Compact)
                                .disabled(action_busy)
                                .on_click({
                                    let state_entity = state_entity.clone();
                                    let message_id = message_id.clone();
                                    move |_, _, app_cx| {
                                        start_message_mutation_and_refresh(
                                            &state_entity,
                                            NativeMessageMutation::Delete {
                                                message_id: message_id.clone(),
                                            },
                                            app_cx,
                                        );
                                    }
                                }),
                        )
                        .when(is_flagged, |row| row.child(reading_badge("Flagged", cx)))
                        .when(has_attachments, |row| {
                            row.child(reading_badge("Attachment", cx))
                        }),
                )
                .child(reading_snooze_controls(
                    state_entity,
                    &message_id,
                    action_busy,
                    snooze_picker,
                    cx,
                )),
        )
        .when_some(body_status, |pane, status| {
            pane.child(
                div()
                    .mx(px(20.0))
                    .mt(px(12.0))
                    .p(px(8.0))
                    .rounded(px(4.0))
                    .bg(colors.neutral)
                    .text_size(px(typography.caption.size))
                    .text_color(colors.on_subtle)
                    .child(status),
            )
        })
        // (d) Blocked-images banner — shown when the message contains external
        // images that have been stripped pending user consent.
        .when(document_has_blocked_images(state), |pane| {
            let state_entity = state_entity.clone();
            pane.child(
                div()
                    .mx(px(20.0))
                    .mt(px(8.0))
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(4.0))
                    .bg(colors.neutral)
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(typography.caption.size))
                            .text_color(colors.on_subtle)
                            .child("External images are blocked."),
                    )
                    .child(
                        Button::new("reading-show-images")
                            .label("Show images")
                            .appearance(ButtonAppearance::Subtle)
                            .size(ButtonSize::Compact)
                            .on_click(move |_, _, app_cx| {
                                toggle_show_external_images(&state_entity, app_cx);
                            }),
                    ),
            )
        })
        .child(
            div()
                .flex_1()
                .min_h_0()
                .px(px(20.0))
                .py(px(16.0))
                .child(DocumentViewer::new(selected_document(state)).max_height(520.0)),
        )
        .when(has_attachments, |pane| {
            pane.child(reading_attachment_section(
                state_entity,
                &message_id,
                &attachments,
                state.attachment_busy,
                state.attachment_notice.as_deref(),
                state.attachment_error.as_deref(),
                cx,
            ))
        })
        .child(
            div()
                .border_t_1()
                .border_color(colors.stroke_neutral_subtle)
                .px(px(20.0))
                .py(px(12.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .w(px(34.0))
                                .text_size(px(typography.caption.size))
                                .text_color(colors.on_subtle)
                                .child("To"),
                        )
                        .child(div().flex_1().min_w_0().child(recipient_input.clone())),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().flex_1().min_w_0().child(quick_reply_input.clone()))
                        .child(
                            Button::new("reading-send-quick-reply")
                                .label(if state.compose.sending {
                                    "Sending..."
                                } else {
                                    "Send"
                                })
                                .appearance(ButtonAppearance::Accent)
                                .size(ButtonSize::Compact)
                                .disabled(!can_send_quick_reply)
                                .on_click({
                                    let state_entity = state_entity.clone();
                                    let recipient_input = recipient_input.clone();
                                    let quick_reply_input = quick_reply_input.clone();
                                    move |_, _, app_cx| {
                                        start_quick_reply_send(
                                            &state_entity,
                                            recipient_input.clone(),
                                            quick_reply_input.clone(),
                                            app_cx,
                                        );
                                    }
                                }),
                        ),
                )
                .when_some(compose_status, |footer, (status, is_error)| {
                    footer.child(
                        div()
                            .text_size(px(typography.caption.size))
                            .text_color(if is_error {
                                colors.status_error
                            } else {
                                colors.on_subtle
                            })
                            .child(status),
                    )
                }),
        )
}

pub(crate) fn selected_document(state: &NativeAppState) -> RichDocument {
    if let Some(message) = state.selected_full_message.as_ref() {
        if let Some(body) = message.body.as_ref() {
            if let Some(html) = body
                .html_body
                .as_ref()
                .filter(|html| !html.trim().is_empty())
            {
                let mut doc = RichDocument::from_html_fragment_lossy(html);
                if !state.show_external_images {
                    strip_external_images(&mut doc);
                }
                return doc;
            }
            if let Some(text) = body
                .text_body
                .as_ref()
                .filter(|text| !text.trim().is_empty())
            {
                return RichDocument::from_plain_text(text);
            }
        }
        return RichDocument::from_plain_text(
            message
                .preview
                .as_deref()
                .unwrap_or("No message body was loaded."),
        );
    }

    let preview = state
        .selected_message
        .as_deref()
        .and_then(|id| state.messages.iter().find(|message| message.id == id))
        .and_then(|message| message.preview.as_deref())
        .unwrap_or("Select a message to load its body.");
    RichDocument::from_plain_text(preview)
}

/// Returns true if the full message body contains external (http/https) images
/// that are currently blocked.
pub(crate) fn document_has_blocked_images(state: &NativeAppState) -> bool {
    if state.show_external_images {
        return false;
    }
    if let Some(message) = state.selected_full_message.as_ref() {
        if let Some(body) = message.body.as_ref() {
            if let Some(html) = body.html_body.as_ref() {
                let doc = RichDocument::from_html_fragment_lossy(html);
                return doc.blocks.iter().any(|block| {
                    matches!(block, RichDocumentBlock::Image { src, .. }
                        if src.starts_with("http://") || src.starts_with("https://"))
                });
            }
        }
    }
    false
}

/// Removes external image blocks from the document in place so they are not
/// rendered until the user explicitly requests them.
pub(crate) fn strip_external_images(doc: &mut RichDocument) {
    doc.blocks.retain(|block| {
        !matches!(block, RichDocumentBlock::Image { src, .. }
            if src.starts_with("http://") || src.starts_with("https://"))
    });
}

pub(crate) fn toggle_show_external_images(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        state.show_external_images = true;
        cx.notify();
    });
}

pub(crate) fn default_snooze_value() -> DateTimeValue {
    let tomorrow = Local::now() + ChronoDuration::days(1);
    DateTimeValue::new(
        tomorrow.year(),
        tomorrow.month() as u8,
        tomorrow.day() as u8,
        9,
        0,
    )
    .unwrap_or(DateTimeValue {
        year: 2000,
        month: 1,
        day: 1,
        hour: 9,
        minute: 0,
    })
}

pub(crate) fn snooze_preset_value_at(
    preset: SnoozePreset,
    now: chrono::DateTime<Local>,
) -> DateTimeValue {
    let target = match preset {
        SnoozePreset::LaterToday => {
            let later = now + ChronoDuration::hours(3);
            DateTimeValue::new(
                later.year(),
                later.month() as u8,
                later.day() as u8,
                later.hour() as u8,
                0,
            )
        }
        SnoozePreset::TomorrowMorning => {
            let tomorrow = now + ChronoDuration::days(1);
            DateTimeValue::new(
                tomorrow.year(),
                tomorrow.month() as u8,
                tomorrow.day() as u8,
                9,
                0,
            )
        }
        SnoozePreset::NextMonday => {
            let days_until_monday = match now.weekday().num_days_from_monday() {
                0 => 7,
                days => 7 - days,
            };
            let next_monday = now + ChronoDuration::days(i64::from(days_until_monday));
            DateTimeValue::new(
                next_monday.year(),
                next_monday.month() as u8,
                next_monday.day() as u8,
                9,
                0,
            )
        }
    };

    target.unwrap_or_else(default_snooze_value)
}
