use crate::prelude::*;
use crate::*;

/// One earlier or later message of the selected conversation, shown as a
/// row beneath the reading pane's selected message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadStackEntry {
    pub(crate) id: String,
    pub(crate) sender: String,
    pub(crate) date: Option<String>,
    pub(crate) preview: String,
    pub(crate) is_read: bool,
    pub(crate) expanded: bool,
}

/// The other messages in the selected message's thread, oldest first.
/// Empty when the selection is a single message or nothing is selected.
pub(crate) fn thread_stack_entries(state: &NativeAppState) -> Vec<ThreadStackEntry> {
    let Some(selected) = selected_summary(state) else {
        return Vec::new();
    };
    let key = message_thread_key(selected);
    let mut siblings: Vec<&Message> = state
        .messages
        .iter()
        .filter(|message| message.id != selected.id && message_thread_key(message) == key)
        .collect();
    siblings.sort_by(|a, b| {
        message_sort_timestamp(a)
            .cmp(&message_sort_timestamp(b))
            .then_with(|| a.id.cmp(&b.id))
    });
    siblings
        .into_iter()
        .map(|message| ThreadStackEntry {
            id: message.id.clone(),
            sender: message
                .from_name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .or(message.from_email.as_deref())
                .unwrap_or("Unknown sender")
                .to_string(),
            date: message.date.clone(),
            preview: message.preview.clone().unwrap_or_default(),
            is_read: message.is_read,
            expanded: state.expanded_stack_message_ids.contains(&message.id),
        })
        .collect()
}

pub(crate) fn thread_stack_header(count: usize) -> String {
    match count {
        1 => "1 other message in this conversation".to_string(),
        n => format!("{n} other messages in this conversation"),
    }
}

/// Label for the bulk toggle: collapse when every row is open, else expand.
pub(crate) fn thread_stack_bulk_label(entries: &[ThreadStackEntry]) -> &'static str {
    if !entries.is_empty() && entries.iter().all(|entry| entry.expanded) {
        "Collapse all"
    } else {
        "Expand all"
    }
}

pub(crate) fn toggle_stack_message(state: &mut NativeAppState, message_id: &str) {
    if !state.expanded_stack_message_ids.remove(message_id) {
        state
            .expanded_stack_message_ids
            .insert(message_id.to_string());
    }
}

pub(crate) fn toggle_all_stack_messages(state: &mut NativeAppState) {
    let entries = thread_stack_entries(state);
    if thread_stack_bulk_label(&entries) == "Collapse all" {
        for entry in &entries {
            state.expanded_stack_message_ids.remove(&entry.id);
        }
    } else {
        state
            .expanded_stack_message_ids
            .extend(entries.into_iter().map(|entry| entry.id));
    }
}

/// Drop expansion flags for messages no longer in the list.
pub(crate) fn retain_existing_stack_expansions(state: &mut NativeAppState) {
    let ids: BTreeSet<&str> = state.messages.iter().map(|m| m.id.as_str()).collect();
    state
        .expanded_stack_message_ids
        .retain(|id| ids.contains(id.as_str()));
}

pub(crate) fn thread_stack_section(
    state_entity: &Entity<NativeAppState>,
    entries: &[ThreadStackEntry],
    cx: &mut App,
) -> impl IntoElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let bulk_label = thread_stack_bulk_label(entries);
    let bulk_entity = state_entity.clone();

    let mut section = div()
        .id("reading-thread-stack")
        .mx(px(20.0))
        .mb(px(8.0))
        .flex()
        .flex_col()
        .border_1()
        .border_color(colors.stroke_neutral_subtle)
        .child(
            div()
                .px(px(12.0))
                .py(px(6.0))
                .flex()
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .text_size(px(typography.caption.size))
                        .text_color(colors.on_subtle)
                        .child(thread_stack_header(entries.len())),
                )
                .child(
                    Button::new("reading-thread-stack-bulk")
                        .label(bulk_label)
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .on_click(move |_, _, app_cx| {
                            bulk_entity.update(app_cx, |state, cx| {
                                toggle_all_stack_messages(state);
                                cx.notify();
                            });
                        }),
                ),
        );
    for (idx, entry) in entries.iter().enumerate() {
        let toggle_entity = state_entity.clone();
        let open_entity = state_entity.clone();
        let toggle_id = entry.id.clone();
        let open_id = entry.id.clone();
        let label = match entry.date.as_deref() {
            Some(date) => format!("{} — {date}", entry.sender),
            None => entry.sender.clone(),
        };
        let mut row = div()
            .px(px(12.0))
            .py(px(6.0))
            .border_t_1()
            .border_color(colors.stroke_neutral_subtle)
            .flex()
            .flex_col()
            .child(
                div().flex().items_center().child(
                    Button::new(("reading-thread-stack-toggle", idx))
                        .label(format!(
                            "{} {}{label}",
                            if entry.expanded { "▾" } else { "▸" },
                            if entry.is_read { "" } else { "● " }
                        ))
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .on_click(move |_, _, app_cx| {
                            toggle_entity.update(app_cx, |state, cx| {
                                toggle_stack_message(state, &toggle_id);
                                cx.notify();
                            });
                        }),
                ),
            );
        if entry.expanded {
            row = row
                .child(
                    div()
                        .py(px(6.0))
                        .text_size(px(typography.body.size))
                        .text_color(colors.on_neutral)
                        .child(if entry.preview.is_empty() {
                            "No preview available".to_string()
                        } else {
                            entry.preview.clone()
                        }),
                )
                .child(
                    div().child(
                        Button::new(("reading-thread-stack-open", idx))
                            .label("Open message")
                            .appearance(ButtonAppearance::Subtle)
                            .size(ButtonSize::Compact)
                            .on_click(move |_, _, app_cx| {
                                start_full_message_load(&open_entity, &open_id, app_cx);
                            }),
                    ),
                );
        }
        section = section.child(row);
    }
    section
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, thread: Option<&str>, date: &str) -> Message {
        Message {
            id: id.to_string(),
            occurrence_id: id.to_string(),
            logical_id: id.to_string(),
            account_id: "acct".to_string(),
            folder_id: "inbox".to_string(),
            uid: 1,
            message_id: None,
            thread_id: thread.map(str::to_string),
            subject: Some("Subject".to_string()),
            from_name: Some(format!("Sender {id}")),
            from_email: Some(format!("{id}@example.com")),
            to_json: None,
            date: Some(date.to_string()),
            is_read: true,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: Some(format!("preview {id}")),
            snoozed_until: None,
        }
    }

    fn state_with_thread() -> NativeAppState {
        NativeAppState {
            messages: vec![
                msg("c", Some("t1"), "2026-01-03T00:00:00Z"),
                msg("other", Some("t2"), "2026-01-02T00:00:00Z"),
                msg("a", Some("t1"), "2026-01-01T00:00:00Z"),
                msg("b", Some("t1"), "2026-01-02T00:00:00Z"),
            ],
            selected_message: Some("c".to_string()),
            ..NativeAppState::default()
        }
    }

    #[test]
    fn lists_thread_siblings_oldest_first_excluding_selected() {
        let ids: Vec<_> = thread_stack_entries(&state_with_thread())
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(ids, ["a", "b"]);
    }

    #[test]
    fn single_message_or_no_selection_has_no_stack() {
        let mut state = state_with_thread();
        state.selected_message = Some("other".to_string());
        assert!(thread_stack_entries(&state).is_empty());
        state.selected_message = None;
        assert!(thread_stack_entries(&state).is_empty());
    }

    #[test]
    fn rows_start_collapsed_and_toggle_individually() {
        let mut state = state_with_thread();
        assert!(thread_stack_entries(&state).iter().all(|e| !e.expanded));
        toggle_stack_message(&mut state, "a");
        let entries = thread_stack_entries(&state);
        assert!(entries[0].expanded && !entries[1].expanded);
        toggle_stack_message(&mut state, "a");
        assert!(thread_stack_entries(&state).iter().all(|e| !e.expanded));
    }

    #[test]
    fn bulk_toggle_expands_then_collapses_all() {
        let mut state = state_with_thread();
        toggle_stack_message(&mut state, "a");
        assert_eq!(
            thread_stack_bulk_label(&thread_stack_entries(&state)),
            "Expand all"
        );
        toggle_all_stack_messages(&mut state);
        assert!(thread_stack_entries(&state).iter().all(|e| e.expanded));
        assert_eq!(
            thread_stack_bulk_label(&thread_stack_entries(&state)),
            "Collapse all"
        );
        toggle_all_stack_messages(&mut state);
        assert!(thread_stack_entries(&state).iter().all(|e| !e.expanded));
    }

    #[test]
    fn missing_thread_id_falls_back_to_own_key() {
        let state = NativeAppState {
            selected_message: Some("x".to_string()),
            messages: vec![
                msg("x", None, "2026-01-01T00:00:00Z"),
                msg("y", None, "2026-01-02T00:00:00Z"),
            ],
            ..NativeAppState::default()
        };
        assert!(thread_stack_entries(&state).is_empty());
    }

    #[test]
    fn stale_expansions_are_dropped() {
        let mut state = state_with_thread();
        state.expanded_stack_message_ids.insert("gone".to_string());
        state.expanded_stack_message_ids.insert("a".to_string());
        retain_existing_stack_expansions(&mut state);
        assert_eq!(
            state.expanded_stack_message_ids.iter().collect::<Vec<_>>(),
            ["a"]
        );
    }

    #[test]
    fn header_pluralises() {
        assert_eq!(
            thread_stack_header(1),
            "1 other message in this conversation"
        );
        assert_eq!(
            thread_stack_header(3),
            "3 other messages in this conversation"
        );
    }
}
