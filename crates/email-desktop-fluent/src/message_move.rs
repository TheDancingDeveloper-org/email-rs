//! Moving messages onto sidebar labels: pointer drag-and-drop and the
//! keyboard "Move to..." alternative.  Both paths end in
//! [`start_label_drop`], which queues the change through the shared undo slot.
//!
//! The server has no move-to-folder operation, so only labels are targets.

use crate::prelude::*;
use crate::*;

/// What a message-row drag carries: the dragged row, or the whole selection
/// when the dragged row is part of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MessageDragPayload {
    pub(crate) message_ids: Vec<String>,
}

pub(crate) struct MessageDragGhost {
    text: String,
}

impl MessageDragGhost {
    pub(crate) fn new(payload: &MessageDragPayload) -> Self {
        let count = payload.message_ids.len();
        Self {
            text: if count == 1 {
                "1 message".to_string()
            } else {
                format!("{count} messages")
            },
        }
    }
}

impl Render for MessageDragGhost {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        div()
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(4.0))
            .bg(colors.accent)
            .text_color(colors.on_accent)
            .child(self.text.clone())
    }
}

/// The messages a drag (or "Move to...") started on `row_id` acts on.
pub(crate) fn move_payload_for_row(state: &NativeAppState, row_id: &str) -> MessageDragPayload {
    let selected = selected_action_message_ids(state, None);
    let message_ids = if selected.iter().any(|id| id == row_id) {
        selected
    } else {
        vec![row_id.to_string()]
    };
    MessageDragPayload { message_ids }
}

/// Whether `label_id` may receive `payload`.
pub(crate) fn label_drop_accepts(
    payload: &MessageDragPayload,
    label_id: &str,
    labels: &[Label],
) -> bool {
    !payload.message_ids.is_empty() && labels.iter().any(|label| label.id == label_id)
}

/// Queue `payload` for `label_id`.  Returns `false` when the drop is rejected.
pub(crate) fn start_label_drop(
    state_entity: &Entity<NativeAppState>,
    payload: &MessageDragPayload,
    label_id: &str,
    app_cx: &mut App,
) -> bool {
    let accepted = state_entity.update(app_cx, |state, cx| {
        if !label_drop_accepts(payload, label_id, &state.labels) {
            return false;
        }
        state.selected_message_ids.clear();
        cx.notify();
        true
    });
    if !accepted {
        return false;
    }
    start_message_mutation_and_refresh(
        state_entity,
        NativeMessageMutation::AddLabels {
            message_ids: payload.message_ids.clone(),
            label_id: label_id.to_string(),
        },
        app_cx,
    );
    true
}

/// Keyboard path: apply `label_id` to the selection, or to the focused row
/// when nothing is selected.
pub(crate) fn start_label_move_for_selection(
    state_entity: &Entity<NativeAppState>,
    focused_id: Option<&str>,
    label_id: &str,
    app_cx: &mut App,
) -> bool {
    let state = state_entity.read(app_cx).clone();
    let row_id = if state.selected_message_ids.is_empty() {
        focused_id
    } else {
        state.selected_message_ids.first().map(String::as_str)
    };
    let Some(row_id) = row_id else {
        return false;
    };
    let payload = move_payload_for_row(&state, row_id);
    start_label_drop(state_entity, &payload, label_id, app_cx)
}

/// The "Move to..." menu, one entry per label.
pub(crate) fn move_to_menu(
    state_entity: &Entity<NativeAppState>,
    focused_id: Option<String>,
    position: Point<Pixels>,
    app_cx: &mut App,
) -> Entity<ContextMenu> {
    let labels = state_entity.read(app_cx).labels.clone();
    let state_entity = state_entity.clone();
    app_cx.new(move |_| {
        let mut menu = ContextMenu::build(position);
        for label in labels {
            let state_entity = state_entity.clone();
            let focused_id = focused_id.clone();
            let label_id = label.id.clone();
            menu = menu.action(label.name.clone(), move |_, _, app_cx| {
                start_label_move_for_selection(
                    &state_entity,
                    focused_id.as_deref(),
                    &label_id,
                    app_cx,
                );
            });
        }
        menu
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(id: &str) -> Label {
        Label {
            id: id.to_string(),
            account_id: "account".to_string(),
            name: id.to_string(),
            color: "#5b5fc7".to_string(),
        }
    }

    fn message(id: &str) -> Message {
        Message {
            id: id.to_string(),
            occurrence_id: id.to_string(),
            logical_id: format!("logical-{id}"),
            account_id: "account".to_string(),
            folder_id: "inbox".to_string(),
            uid: 1,
            message_id: None,
            thread_id: None,
            subject: None,
            from_name: None,
            from_email: None,
            to_json: None,
            date: None,
            is_read: true,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: None,
            snoozed_until: None,
        }
    }

    fn state(selected: &[&str]) -> NativeAppState {
        NativeAppState {
            messages: ["m1", "m2", "m3"].map(message).to_vec(),
            labels: vec![label("work")],
            selected_message_ids: selected.iter().map(|id| id.to_string()).collect(),
            ..Default::default()
        }
    }

    fn payload(ids: &[&str]) -> MessageDragPayload {
        MessageDragPayload {
            message_ids: ids.iter().map(|id| id.to_string()).collect(),
        }
    }

    #[test]
    fn drop_target_accepts_known_label_and_rejects_the_rest() {
        let labels = [label("work")];
        assert!(label_drop_accepts(&payload(&["m1"]), "work", &labels));
        assert!(!label_drop_accepts(&payload(&["m1"]), "gone", &labels));
        assert!(!label_drop_accepts(&payload(&[]), "work", &labels));
    }

    #[test]
    fn dragging_a_selected_row_carries_the_whole_selection() {
        let state = state(&["m1", "m3"]);
        assert_eq!(move_payload_for_row(&state, "m3"), payload(&["m1", "m3"]));
    }

    #[test]
    fn dragging_an_unselected_row_carries_only_that_row() {
        let state = state(&["m1", "m3"]);
        assert_eq!(move_payload_for_row(&state, "m2"), payload(&["m2"]));
    }

    #[gpui::test]
    fn multi_select_drop_queues_one_undoable_label_mutation(cx: &mut gpui::TestAppContext) {
        fluent_test_utils::init_fluent_test(cx);
        let entity = cx.new(|_| state(&["m1", "m3"]));
        let accepted =
            cx.update(|app| start_label_drop(&entity, &payload(&["m1", "m3"]), "work", app));
        assert!(accepted);
        entity.update(cx, |state, _| {
            assert!(state.selected_message_ids.is_empty());
            assert_eq!(
                state.pending_undo.as_ref().map(|p| p.mutation.clone()),
                Some(NativeMessageMutation::AddLabels {
                    message_ids: vec!["m1".to_string(), "m3".to_string()],
                    label_id: "work".to_string(),
                })
            );
        });
    }

    #[gpui::test]
    fn rejected_drop_changes_nothing(cx: &mut gpui::TestAppContext) {
        fluent_test_utils::init_fluent_test(cx);
        let entity = cx.new(|_| state(&["m1"]));
        let accepted = cx.update(|app| start_label_drop(&entity, &payload(&["m1"]), "gone", app));
        assert!(!accepted);
        entity.update(cx, |state, _| {
            assert!(state.pending_undo.is_none());
            assert_eq!(state.selected_message_ids, vec!["m1".to_string()]);
        });
    }

    #[gpui::test]
    fn keyboard_move_uses_selection_else_focused_row(cx: &mut gpui::TestAppContext) {
        fluent_test_utils::init_fluent_test(cx);
        let entity = cx.new(|_| state(&["m1", "m2"]));
        assert!(cx.update(|app| start_label_move_for_selection(&entity, Some("m3"), "work", app)));
        entity.update(cx, |state, _| {
            assert_eq!(
                state.pending_undo.as_ref().map(|p| p.mutation.clone()),
                Some(NativeMessageMutation::AddLabels {
                    message_ids: vec!["m1".to_string(), "m2".to_string()],
                    label_id: "work".to_string(),
                })
            );
        });

        let entity = cx.new(|_| state(&[]));
        assert!(cx.update(|app| start_label_move_for_selection(&entity, Some("m3"), "work", app)));
        entity.update(cx, |state, _| {
            assert_eq!(
                state
                    .pending_undo
                    .as_ref()
                    .map(|p| p.mutation.message_id().to_string()),
                Some("m3".to_string())
            );
        });
        assert!(!cx.update(|app| start_label_move_for_selection(&entity, None, "work", app)));
    }

    #[gpui::test]
    fn undo_cancels_the_pending_label_without_committing(cx: &mut gpui::TestAppContext) {
        fluent_test_utils::init_fluent_test(cx);
        let entity = cx.new(|_| state(&[]));
        cx.update(|app| {
            start_label_drop(&entity, &payload(&["m1"]), "work", app);
            cancel_pending_undo(&entity, app);
        });
        entity.update(cx, |state, _| assert!(state.pending_undo.is_none()));
    }
}
