use crate::prelude::*;
use crate::*;

#[derive(Clone)]
pub(crate) struct MessageListPlaceholder {
    pub(crate) state: Entity<NativeAppState>,
    pub(crate) context_menu: Option<Entity<ContextMenu>>,
}

impl MessageListPlaceholder {
    pub(crate) fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            context_menu: None,
        }
    }
}

impl Render for MessageListPlaceholder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let typography = theme.typography;
        let mailbox_title = current_mailbox_title(&state);
        let message_count = state.messages.len();
        let unread_count = state
            .messages
            .iter()
            .filter(|message| !message.is_read)
            .count();
        let selected_count = state.selected_message_ids.len();
        let mailbox_summary = if state.messages_resource.is_loading() {
            format!("{message_count} cached messages - loading")
        } else if let Some(error) = state.messages_resource.error() {
            format!("{message_count} cached messages - {error}")
        } else if selected_count > 0 {
            format!("{selected_count} selected")
        } else {
            format!("{message_count} messages - {unread_count} unread")
        };
        let can_mark_all_read =
            selected_count == 0 && selected_regular_folder_id(&state).is_some() && unread_count > 0;
        let select_all_state = message_select_all_state(&state);

        let mut list = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors.surface)
            .child(
                div()
                    .px(px(18.0))
                    .pt(px(14.0))
                    .pb(px(12.0))
                    .border_b_1()
                    .border_color(colors.stroke_neutral_subtle)
                    .child(
                        div()
                            .text_size(px(typography.subtitle.size))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.on_neutral)
                            .child(mailbox_title),
                    )
                    .child(
                        div()
                            .pt(px(4.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                Checkbox::new("message-list-select-all")
                                    .state(select_all_state)
                                    .disabled(state.messages.is_empty())
                                    .on_click({
                                        let state_entity = self.state.clone();
                                        move |next, _, _, app_cx| {
                                            app_cx.stop_propagation();
                                            state_entity.update(app_cx, |state, cx| {
                                                set_all_message_selection(
                                                    state,
                                                    matches!(next, CheckboxState::Checked),
                                                );
                                                cx.notify();
                                            });
                                        }
                                    }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(typography.caption.size))
                                    .text_color(if state.messages_resource.is_error() {
                                        colors.status_error
                                    } else {
                                        colors.on_subtle
                                    })
                                    .child(mailbox_summary),
                            )
                            .when(can_mark_all_read, |row| {
                                row.child(
                                    Button::new("message-list-mark-all-read")
                                        .label("Mark all read")
                                        .appearance(ButtonAppearance::Subtle)
                                        .size(ButtonSize::Compact)
                                        .on_click({
                                            let state_entity = self.state.clone();
                                            let folder_id = selected_regular_folder_id(&state)
                                                .unwrap()
                                                .to_string();
                                            move |_, _, app_cx| {
                                                start_mark_folder_read(
                                                    &state_entity,
                                                    folder_id.clone(),
                                                    app_cx,
                                                );
                                            }
                                        }),
                                )
                            })
                            .when(selected_count > 0, |row| {
                                row.child(
                                    Button::new("message-list-bulk-archive")
                                        .label("Archive")
                                        .appearance(ButtonAppearance::Subtle)
                                        .size(ButtonSize::Compact)
                                        .on_click({
                                            let state_entity = self.state.clone();
                                            move |_, _, app_cx| {
                                                start_bulk_message_action(
                                                    &state_entity,
                                                    BulkAction::Archive,
                                                    app_cx,
                                                );
                                            }
                                        }),
                                )
                                .child(
                                    Button::new("message-list-bulk-read")
                                        .label("Mark read")
                                        .appearance(ButtonAppearance::Subtle)
                                        .size(ButtonSize::Compact)
                                        .on_click({
                                            let state_entity = self.state.clone();
                                            move |_, _, app_cx| {
                                                start_bulk_message_action(
                                                    &state_entity,
                                                    BulkAction::MarkRead,
                                                    app_cx,
                                                );
                                            }
                                        }),
                                )
                                .child(
                                    Button::new("message-list-bulk-unread")
                                        .label("Mark unread")
                                        .appearance(ButtonAppearance::Subtle)
                                        .size(ButtonSize::Compact)
                                        .on_click({
                                            let state_entity = self.state.clone();
                                            move |_, _, app_cx| {
                                                start_bulk_message_action(
                                                    &state_entity,
                                                    BulkAction::MarkUnread,
                                                    app_cx,
                                                );
                                            }
                                        }),
                                )
                                .child(
                                    Button::new("message-list-bulk-delete")
                                        .label("Delete")
                                        .appearance(ButtonAppearance::Danger)
                                        .size(ButtonSize::Compact)
                                        .on_click({
                                            let state_entity = self.state.clone();
                                            move |_, _, app_cx| {
                                                start_bulk_message_action(
                                                    &state_entity,
                                                    BulkAction::Delete,
                                                    app_cx,
                                                );
                                            }
                                        }),
                                )
                                .child(
                                    Button::new("message-list-clear-selection")
                                        .label("Clear")
                                        .appearance(ButtonAppearance::Subtle)
                                        .size(ButtonSize::Compact)
                                        .on_click({
                                            let state_entity = self.state.clone();
                                            move |_, _, app_cx| {
                                                state_entity.update(app_cx, |state, cx| {
                                                    state.selected_message_ids.clear();
                                                    cx.notify();
                                                });
                                            }
                                        }),
                                )
                            }),
                    ),
            )
            .when_some(state.pending_send.as_ref(), |list, send| {
                let cancelling = send.cancelling;
                list.child(
                    div()
                        .mx(px(12.0))
                        .mt(px(8.0))
                        .p(px(8.0))
                        .rounded(px(4.0))
                        .bg(colors.neutral)
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(typography.caption.size))
                                .text_color(colors.on_neutral)
                                .child(if cancelling {
                                    "Cancelling send..."
                                } else {
                                    "Message sent"
                                }),
                        )
                        .child(
                            Button::new("send-undo")
                                .label("Undo")
                                .appearance(ButtonAppearance::Subtle)
                                .size(ButtonSize::Compact)
                                .disabled(cancelling)
                                .on_click({
                                    let state_entity = self.state.clone();
                                    move |_, _, app_cx| start_cancel_send(&state_entity, app_cx)
                                }),
                        )
                        .child(
                            Button::new("send-undo-dismiss")
                                .label("Dismiss")
                                .appearance(ButtonAppearance::Subtle)
                                .size(ButtonSize::Compact)
                                .on_click({
                                    let state_entity = self.state.clone();
                                    move |_, _, app_cx| dismiss_pending_send(&state_entity, app_cx)
                                }),
                        ),
                )
            })
            .when_some(state.move_notice.as_ref(), |list, notice| {
                let can_undo = notice.summary.can_undo();
                list.child(
                    div()
                        .mx(px(12.0))
                        .mt(px(8.0))
                        .p(px(8.0))
                        .rounded(px(4.0))
                        .bg(colors.neutral)
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(typography.caption.size))
                                .text_color(if notice.summary.failed.is_empty() {
                                    colors.on_neutral
                                } else {
                                    colors.status_error
                                })
                                .child(notice.summary.text()),
                        )
                        .when(can_undo, |row| {
                            row.child(
                                Button::new("move-notice-undo")
                                    .label("Undo")
                                    .appearance(ButtonAppearance::Subtle)
                                    .size(ButtonSize::Compact)
                                    .on_click({
                                        let state_entity = self.state.clone();
                                        move |_, _, app_cx| {
                                            start_undo_completed_move(&state_entity, app_cx)
                                        }
                                    }),
                            )
                        })
                        .child(
                            Button::new("move-notice-dismiss")
                                .label("Dismiss")
                                .appearance(ButtonAppearance::Subtle)
                                .size(ButtonSize::Compact)
                                .on_click({
                                    let state_entity = self.state.clone();
                                    move |_, _, app_cx| dismiss_move_notice(&state_entity, app_cx)
                                }),
                        ),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(mail_message_list(&self.state, &state, cx)),
            );

        if let Some(menu_entity) = &self.context_menu {
            if menu_entity.read(cx).open {
                list = list.child(menu_entity.clone());
            }
        }

        list
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MessageThreadGroup {
    pub(crate) key: String,
    pub(crate) messages: Vec<Message>,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum MessageListRenderRow {
    Message {
        message: Message,
        thread_key: Option<String>,
        thread_count: usize,
        indented: bool,
    },
    CollapseThread {
        thread_key: String,
    },
}

impl MessageListRenderRow {
    pub(crate) fn message_id(&self) -> Option<&str> {
        match self {
            Self::Message { message, .. } => Some(message.id.as_str()),
            Self::CollapseThread { .. } => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn thread_count(&self) -> Option<usize> {
        match self {
            Self::Message { thread_count, .. } if *thread_count > 1 => Some(*thread_count),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn is_indented(&self) -> bool {
        matches!(self, Self::Message { indented: true, .. })
    }
}

pub(crate) fn message_thread_key(message: &Message) -> String {
    message
        .thread_id
        .as_deref()
        .map(str::trim)
        .filter(|thread_id| !thread_id.is_empty())
        .unwrap_or(message.id.as_str())
        .to_string()
}

pub(crate) fn message_sort_timestamp(message: &Message) -> i64 {
    message
        .date
        .as_deref()
        .and_then(|date| chrono::DateTime::parse_from_rfc3339(date).ok())
        .map(|date| date.timestamp_millis())
        .unwrap_or_default()
}

pub(crate) fn group_messages_by_thread(messages: &[Message]) -> Vec<MessageThreadGroup> {
    let mut order = Vec::new();
    let mut groups = BTreeMap::<String, Vec<Message>>::new();
    for message in messages {
        let key = message_thread_key(message);
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(message.clone());
    }

    order
        .into_iter()
        .filter_map(|key| {
            let mut messages = groups.remove(&key)?;
            messages.sort_by(|a, b| {
                message_sort_timestamp(b)
                    .cmp(&message_sort_timestamp(a))
                    .then_with(|| a.id.cmp(&b.id))
            });
            Some(MessageThreadGroup { key, messages })
        })
        .collect()
}

pub(crate) fn message_list_uses_threading(state: &NativeAppState) -> bool {
    selected_regular_folder_id(state).is_some()
}

pub(crate) fn message_list_render_rows(state: &NativeAppState) -> Vec<MessageListRenderRow> {
    if !message_list_uses_threading(state) {
        return state
            .messages
            .iter()
            .cloned()
            .map(|message| MessageListRenderRow::Message {
                message,
                thread_key: None,
                thread_count: 1,
                indented: false,
            })
            .collect();
    }

    let selected_message = state.selected_message.as_deref();
    let mut rows = Vec::new();
    for group in group_messages_by_thread(&state.messages) {
        let thread_count = group.messages.len();
        let expanded = thread_count > 1
            && (state.expanded_thread_keys.contains(&group.key)
                || group
                    .messages
                    .iter()
                    .skip(1)
                    .any(|message| selected_message == Some(message.id.as_str())));

        if let Some(latest) = group.messages.first().cloned() {
            rows.push(MessageListRenderRow::Message {
                message: latest,
                thread_key: Some(group.key.clone()),
                thread_count,
                indented: false,
            });
        }

        if expanded {
            for message in group.messages.iter().skip(1).cloned() {
                rows.push(MessageListRenderRow::Message {
                    message,
                    thread_key: Some(group.key.clone()),
                    thread_count,
                    indented: true,
                });
            }
            rows.push(MessageListRenderRow::CollapseThread {
                thread_key: group.key,
            });
        }
    }

    rows
}

pub(crate) fn retain_existing_thread_expansions(state: &mut NativeAppState) {
    if !message_list_uses_threading(state) {
        state.expanded_thread_keys.clear();
        return;
    }

    let visible_threads = group_messages_by_thread(&state.messages)
        .into_iter()
        .filter(|group| group.messages.len() > 1)
        .map(|group| group.key)
        .collect::<BTreeSet<_>>();
    state
        .expanded_thread_keys
        .retain(|key| visible_threads.contains(key));
}

pub(crate) fn mail_message_list(
    state_entity: &Entity<NativeAppState>,
    state: &NativeAppState,
    cx: &mut Context<MessageListPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let row_height = state.density.message_row_height.max(66.0);
    let render_rows = message_list_render_rows(state);
    let selected_id = state.selected_message.clone().or_else(|| {
        render_rows
            .iter()
            .find_map(|row| row.message_id().map(str::to_string))
    });
    let keyboard_ids = render_rows
        .iter()
        .filter_map(|row| row.message_id().map(str::to_string))
        .collect::<Vec<_>>();

    let list_for_keys = cx.entity();
    let mut root = div()
        .tab_index(0)
        .key_context("NativeMessageList")
        .flex()
        .flex_col()
        .w_full()
        .h_full()
        .min_h_0()
        .bg(colors.surface)
        .on_key_down({
            let ids = keyboard_ids;
            let selected_id = selected_id.clone();
            let state_entity = state_entity.clone();
            move |ev: &KeyDownEvent, window, app_cx| {
                if matches!(ev.keystroke.key.as_str(), "m" | "v")
                    && !ev.keystroke.modifiers.control
                    && !ev.keystroke.modifiers.platform
                    && !ev.keystroke.modifiers.alt
                {
                    let menu = move_to_menu(
                        &state_entity,
                        selected_id.clone(),
                        window.mouse_position(),
                        app_cx,
                    );
                    list_for_keys.update(app_cx, |list, cx| {
                        list.context_menu = Some(menu);
                        cx.notify();
                    });
                    app_cx.stop_propagation();
                    return;
                }
                if handle_message_list_key(
                    &state_entity,
                    &ids,
                    selected_id.as_deref(),
                    ev.keystroke.key.as_str(),
                    ev.keystroke.modifiers.control || ev.keystroke.modifiers.platform,
                    app_cx,
                ) {
                    app_cx.stop_propagation();
                    return;
                }

                if let Some(target) =
                    message_key_target(&ids, selected_id.as_deref(), ev.keystroke.key.as_str())
                {
                    start_full_message_load(&state_entity, &target, app_cx);
                }
            }
        });

    if let Some((message, error)) = message_list_status(state) {
        let color = if error {
            colors.status_error
        } else {
            colors.on_subtle_disabled
        };
        return root.child(
            div()
                .flex_1()
                .min_h(px(row_height * 2.0))
                .flex()
                .items_center()
                .justify_center()
                .px(px(18.0))
                .text_size(px(typography.body.size))
                .text_color(color)
                .child(message),
        );
    }

    let search_events = state.search_event_results.clone();
    let selected_id_for_rows = selected_id.clone();
    let selected_ids_for_rows = state
        .selected_message_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let row_state = state_entity.clone();
    let list_entity = cx.entity();
    let mut list_body = div().flex_1().min_h_0().flex().flex_col();
    if !search_events.is_empty() {
        list_body = list_body.child(calendar_search_results_section(
            state_entity,
            &search_events,
            cx,
        ));
    }
    if !render_rows.is_empty() {
        let rows = render_rows.clone();
        list_body = list_body.child(
            uniform_list(
                "native-mail-message-rows",
                rows.len(),
                move |range, _, app_cx| {
                    range
                        .map(|idx| match &rows[idx] {
                            MessageListRenderRow::Message {
                                message,
                                thread_key,
                                thread_count,
                                indented,
                            } => mail_message_row(
                                idx,
                                message,
                                selected_id_for_rows.as_deref() == Some(message.id.as_str()),
                                selected_ids_for_rows.contains(&message.id),
                                *thread_count,
                                thread_key.clone(),
                                *indented,
                                row_height,
                                row_state.clone(),
                                list_entity.clone(),
                                app_cx,
                            )
                            .into_any_element(),
                            MessageListRenderRow::CollapseThread { thread_key } => {
                                mail_thread_collapse_row(
                                    idx,
                                    thread_key.clone(),
                                    row_height,
                                    row_state.clone(),
                                    app_cx,
                                )
                                .into_any_element()
                            }
                        })
                        .collect::<Vec<_>>()
                },
            )
            .w_full()
            .flex_1()
            .min_h_0(),
        );
    }
    root = root.child(list_body);

    root
}

pub(crate) fn message_list_status(state: &NativeAppState) -> Option<(String, bool)> {
    if initial_accounts_loading(state) {
        return Some(("Loading accounts...".to_string(), false));
    }

    if state.messages_resource.is_loading() && state.messages.is_empty() {
        return Some(("Loading messages...".to_string(), false));
    }

    if let Some(error) = state.messages_resource.error() {
        return Some((format!("Messages failed to load: {error}"), true));
    }

    if !state.messages.is_empty() || !state.search_event_results.is_empty() {
        return None;
    }

    if let Some(query) = state.search.submitted_query.as_deref() {
        return Some((format!("No results for \"{query}\""), false));
    }

    if state.search.condition_group.is_some() {
        return Some(("No advanced search results".to_string(), false));
    }

    if state.selected_folder.is_none() && state.selected_label.is_none() {
        return Some(("No folder selected".to_string(), false));
    }

    Some(("No messages in this view".to_string(), false))
}

pub(crate) fn message_key_target(
    ids: &[String],
    selected_id: Option<&str>,
    key: &str,
) -> Option<String> {
    if ids.is_empty() {
        return None;
    }

    let current = selected_id
        .and_then(|id| ids.iter().position(|message_id| message_id == id))
        .unwrap_or(0);
    let next = match key {
        "up" | "k" => current.saturating_sub(1),
        "down" | "j" => (current + 1).min(ids.len() - 1),
        "home" => 0,
        "end" => ids.len() - 1,
        "pageup" => current.saturating_sub(5),
        "pagedown" => (current + 5).min(ids.len() - 1),
        "return" | "space" => current,
        _ => return None,
    };

    Some(ids[next].clone())
}

pub(crate) fn handle_message_list_key(
    state_entity: &Entity<NativeAppState>,
    ids: &[String],
    selected_id: Option<&str>,
    key: &str,
    command: bool,
    app_cx: &mut App,
) -> bool {
    if !command {
        match key {
            "z" => return undo_latest_action(state_entity, app_cx),
            "escape" if cancel_pending_undo_on_escape(state_entity, app_cx) => return true,
            _ => {}
        }
    }

    if ids.is_empty() {
        return false;
    }

    if command && key == "a" {
        state_entity.update(app_cx, |state, cx| {
            let all_selected = message_select_all_state(state) == CheckboxState::Checked;
            set_all_message_selection(state, !all_selected);
            cx.notify();
        });
        return true;
    }

    match key {
        "escape" => {
            state_entity.update(app_cx, |state, cx| {
                if state.selected_message_ids.is_empty() {
                    state.selected_message = None;
                    state.selected_full_message = None;
                    state.selected_message_resource.reset();
                    state.selected_message_labels.clear();
                    state.selected_message_labels_resource.reset();
                    reset_compose_state(state, None);
                } else {
                    state.selected_message_ids.clear();
                }
                cx.notify();
            });
            true
        }
        "x" => {
            if let Some(selected_id) = selected_id {
                state_entity.update(app_cx, |state, cx| {
                    toggle_message_selection(state, selected_id);
                    cx.notify();
                });
                true
            } else {
                false
            }
        }
        "e" => start_message_list_target_action(
            state_entity,
            selected_id,
            BulkAction::Archive,
            |message_id| NativeMessageMutation::Archive { message_id },
            app_cx,
        ),
        "backspace" | "delete" => start_message_list_target_action(
            state_entity,
            selected_id,
            BulkAction::Delete,
            |message_id| NativeMessageMutation::Delete { message_id },
            app_cx,
        ),
        "u" => start_message_list_read_toggle(state_entity, selected_id, app_cx),
        _ => false,
    }
}

pub(crate) fn start_message_list_target_action(
    state_entity: &Entity<NativeAppState>,
    selected_id: Option<&str>,
    bulk_action: BulkAction,
    single_mutation: impl Fn(String) -> NativeMessageMutation,
    app_cx: &mut App,
) -> bool {
    let state = state_entity.read(app_cx).clone();
    if state.selected_message_ids.is_empty() {
        let Some(message_id) = selected_id.map(str::to_string) else {
            return false;
        };
        start_message_mutation_and_refresh(state_entity, single_mutation(message_id), app_cx);
    } else {
        start_bulk_message_action(state_entity, bulk_action, app_cx);
    }
    true
}

pub(crate) fn start_message_list_read_toggle(
    state_entity: &Entity<NativeAppState>,
    selected_id: Option<&str>,
    app_cx: &mut App,
) -> bool {
    let state = state_entity.read(app_cx).clone();
    if state.selected_message_ids.is_empty() {
        let Some(message_id) = selected_id else {
            return false;
        };
        let is_read = state
            .messages
            .iter()
            .find(|message| message.id == message_id)
            .map(|message| message.is_read)
            .unwrap_or(true);
        start_message_mutation_and_refresh(
            state_entity,
            NativeMessageMutation::SetRead {
                message_id: message_id.to_string(),
                is_read: !is_read,
            },
            app_cx,
        );
    } else {
        let selected_ids = selected_action_message_ids(&state, None)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let any_unread = state
            .messages
            .iter()
            .filter(|message| selected_ids.contains(&message.id))
            .any(|message| !message.is_read);
        start_bulk_message_action(
            state_entity,
            if any_unread {
                BulkAction::MarkRead
            } else {
                BulkAction::MarkUnread
            },
            app_cx,
        );
    }
    true
}

pub(crate) fn message_row_context_menu(
    state_entity: Entity<NativeAppState>,
    message: Message,
    position: Point<Pixels>,
    app_cx: &mut App,
) -> Entity<ContextMenu> {
    let message_id = message.id.clone();
    let open_state = state_entity.clone();
    let reply_state = state_entity.clone();
    let forward_state = state_entity.clone();
    let archive_state = state_entity.clone();
    let delete_state = state_entity.clone();
    let read_state = state_entity.clone();
    let flag_state = state_entity;
    let is_read = message.is_read;
    let is_flagged = message.is_flagged;

    app_cx.new(move |_| {
        ContextMenu::build(position)
            .action("Open", {
                let message_id = message_id.clone();
                move |_, _, app_cx| {
                    start_full_message_load(&open_state, &message_id, app_cx);
                }
            })
            .action("Reply", {
                let message_id = message_id.clone();
                move |_, _, app_cx| {
                    start_full_message_load(&reply_state, &message_id, app_cx);
                    start_reply_draft(&reply_state, app_cx);
                }
            })
            .action("Forward", {
                let message_id = message_id.clone();
                move |_, _, app_cx| {
                    start_full_message_load(&forward_state, &message_id, app_cx);
                    start_forward_draft(&forward_state, app_cx);
                }
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
            .separator()
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
            .action(if is_flagged { "Unflag" } else { "Flag" }, {
                let message_id = message_id.clone();
                move |_, _, app_cx| {
                    start_message_mutation_and_refresh(
                        &flag_state,
                        NativeMessageMutation::SetFlagged {
                            message_id: message_id.clone(),
                            is_flagged: !is_flagged,
                        },
                        app_cx,
                    );
                }
            })
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn mail_message_row(
    idx: usize,
    message: &Message,
    selected: bool,
    checked: bool,
    thread_count: usize,
    thread_key: Option<String>,
    indented: bool,
    row_height: f32,
    state_entity: Entity<NativeAppState>,
    list_entity: Entity<MessageListPlaceholder>,
    cx: &mut App,
) -> impl IntoElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let subject = message
        .subject
        .clone()
        .filter(|subject| !subject.trim().is_empty())
        .unwrap_or_else(|| "(no subject)".to_string());
    let sender = message_sender(message);
    let preview = message.preview.clone().unwrap_or_default();
    let date = message
        .date
        .clone()
        .filter(|date| !date.trim().is_empty())
        .unwrap_or_else(|| "No date".to_string());
    let unread = !message.is_read;
    let message_id = message.id.clone();
    let checkbox_message_id = message.id.clone();
    let click_thread_key = thread_key.filter(|_| thread_count > 1 && !indented);
    let click_state = state_entity.clone();
    let checkbox_state = state_entity.clone();
    let context_message = message.clone();
    let context_state = state_entity.clone();
    let context_list = list_entity.clone();

    let drag_payload = move_payload_for_row(state_entity.read(cx), &message.id);
    drag_source(
        div()
            .h(px(row_height))
            .w_full()
            .flex()
            .items_center()
            .bg(if selected {
                colors.neutral_selected
            } else {
                colors.surface
            })
            .border_b_1()
            .border_color(colors.stroke_neutral_subtle),
        ("native-mail-message-row", idx as u64),
        drag_payload,
        |payload, _, _, app_cx| app_cx.new(|_| MessageDragGhost::new(payload)),
    )
    .cursor_pointer()
    .hover({
        let hover = colors.subtle_hover;
        let selected_bg = colors.neutral_selected;
        move |style| style.bg(if selected { selected_bg } else { hover })
    })
    .on_click(move |_, _, app_cx| {
        open_message_list_row(&click_state, &message_id, click_thread_key.clone(), app_cx);
    })
    .on_mouse_down(MouseButton::Right, move |ev, _, app_cx| {
        let menu = message_row_context_menu(
            context_state.clone(),
            context_message.clone(),
            ev.position,
            app_cx,
        );
        context_list.update(app_cx, |list, cx| {
            list.context_menu = Some(menu);
            cx.notify();
        });
        app_cx.stop_propagation();
    })
    .child(div().h_full().w(px(3.0)).bg(if selected || unread {
        colors.accent
    } else {
        colors.surface
    }))
    .child(
        div()
            .pl(px(if indented { 28.0 } else { 10.0 }))
            .pr(px(10.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .min_w_0()
            .flex_1()
            .child(
                Checkbox::new(("message-row-select", idx))
                    .checked(checked)
                    .on_click({
                        move |_, _, _, app_cx| {
                            app_cx.stop_propagation();
                            checkbox_state.update(app_cx, |state, cx| {
                                toggle_message_selection(state, &checkbox_message_id);
                                cx.notify();
                            });
                        }
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .size(px(30.0))
                    .rounded(px(999.0))
                    .bg(if unread {
                        colors.accent
                    } else {
                        colors.neutral
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if unread {
                        colors.on_accent
                    } else {
                        colors.on_subtle
                    })
                    .child(sender_initials(&sender)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(typography.body.size))
                                    .font_weight(if unread {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .text_color(colors.on_neutral)
                                    .child(sender),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .max_w(px(92.0))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(typography.caption.size))
                                    .text_color(colors.on_subtle)
                                    .child(date),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(typography.body.size))
                                    .font_weight(if unread {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .text_color(colors.on_neutral)
                                    .child(subject),
                            )
                            .when(message.is_flagged, |row| {
                                row.child(message_row_badge("Flagged", cx))
                            })
                            .when(message.has_attachments, |row| {
                                row.child(message_row_badge("Attachment", cx))
                            })
                            .when(message.snoozed_until.is_some(), |row| {
                                row.child(message_row_badge("Snoozed", cx))
                            })
                            .when(thread_count > 1 && !indented, |row| {
                                row.child(message_row_count_badge(thread_count, cx))
                            }),
                    )
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(typography.caption.size))
                            .text_color(colors.on_subtle)
                            .child(preview),
                    ),
            ),
    )
}

pub(crate) fn open_message_list_row(
    state_entity: &Entity<NativeAppState>,
    message_id: &str,
    thread_key: Option<String>,
    app_cx: &mut App,
) {
    if let Some(thread_key) = thread_key {
        state_entity.update(app_cx, |state, cx| {
            state.expanded_thread_keys.insert(thread_key);
            cx.notify();
        });
    }
    start_full_message_load(state_entity, message_id, app_cx);
}

pub(crate) fn mail_thread_collapse_row(
    idx: usize,
    thread_key: String,
    row_height: f32,
    state_entity: Entity<NativeAppState>,
    cx: &mut App,
) -> impl IntoElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;

    div()
        .id(("native-mail-thread-collapse-row", idx as u64))
        .h(px((row_height * 0.52).max(34.0)))
        .w_full()
        .pl(px(54.0))
        .pr(px(12.0))
        .flex()
        .items_center()
        .border_b_1()
        .border_color(colors.stroke_neutral_subtle)
        .bg(colors.surface)
        .child(
            Button::new(("native-mail-thread-collapse", idx))
                .label("Collapse thread")
                .size(ButtonSize::Compact)
                .appearance(ButtonAppearance::Subtle)
                .on_click(move |_, _, app_cx| {
                    state_entity.update(app_cx, |state, cx| {
                        state.expanded_thread_keys.remove(&thread_key);
                        cx.notify();
                    });
                }),
        )
        .child(
            div()
                .ml(px(8.0))
                .text_size(px(typography.caption.size))
                .text_color(colors.on_subtle)
                .child("Thread replies are hidden until expanded"),
        )
}

pub(crate) fn message_row_badge(label: &'static str, cx: &mut App) -> gpui::Div {
    let theme = cx.theme();
    div()
        .flex_none()
        .px(px(5.0))
        .py(px(1.0))
        .rounded(px(3.0))
        .bg(theme.colors.neutral)
        .text_size(px(10.0))
        .text_color(theme.colors.on_subtle)
        .child(label)
}

pub(crate) fn message_row_count_badge(count: usize, cx: &mut App) -> gpui::Div {
    let theme = cx.theme();
    div()
        .flex_none()
        .min_w(px(18.0))
        .px(px(5.0))
        .py(px(1.0))
        .rounded(px(999.0))
        .bg(theme.colors.accent)
        .text_size(px(10.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.colors.on_accent)
        .child(count.to_string())
}

pub(crate) fn message_sender(message: &Message) -> String {
    message
        .from_name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| message.from_email.clone())
        .unwrap_or_else(|| "Unknown sender".to_string())
}

pub(crate) fn sender_initials(sender: &str) -> String {
    let mut initials = sender
        .split_whitespace()
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect::<String>();
    if initials.is_empty() {
        initials = sender.chars().take(1).collect();
    }
    if initials.is_empty() {
        "U".to_string()
    } else {
        initials.to_uppercase()
    }
}

pub(crate) fn selected_regular_folder_id(state: &NativeAppState) -> Option<&str> {
    if state.search.submitted_query.is_some()
        || state.search.condition_group.is_some()
        || state.selected_label.is_some()
    {
        return None;
    }

    state
        .selected_folder
        .as_deref()
        .filter(|folder_id| !folder_id.starts_with("smart:"))
}

pub(crate) fn current_mailbox_title(state: &NativeAppState) -> String {
    if state.selected_view == SelectedView::Calendar {
        return "Calendar".to_string();
    }

    if let Some(query) = state.search.submitted_query.as_deref() {
        return format!("Results for \"{query}\"");
    }

    if state.search.condition_group.is_some() {
        return "Advanced search results".to_string();
    }

    if let Some(label_id) = state.selected_label.as_deref() {
        return state
            .labels
            .iter()
            .find(|label| label.id == label_id)
            .map(|label| label.name.clone())
            .unwrap_or_else(|| "Label".to_string());
    }

    if let Some(folder_id) = state.selected_folder.as_deref() {
        if let Some(kind) = folder_id.strip_prefix("smart:") {
            return smart_folder_title(kind).to_string();
        }

        if let Some(folder) = state
            .folders_by_account
            .values()
            .flat_map(|folders| folders.iter())
            .find(|folder| folder.id == folder_id)
        {
            return folder_label(folder);
        }
    }

    "Mail".to_string()
}

pub(crate) fn smart_folder_title(kind: &str) -> &'static str {
    match kind {
        "all" => "All Inboxes",
        "unread" => "Unread",
        "flagged" => "Flagged",
        "snoozed" => "Snoozed",
        _ => "Smart Folder",
    }
}
