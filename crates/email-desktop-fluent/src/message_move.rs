//! Moving messages onto sidebar labels: pointer drag-and-drop and the
//! keyboard "Move to..." alternative.  Both paths end in
//! [`start_label_drop`], which queues the change through the shared undo slot.
//!
//! Labels are applied; folders receive a real move (`POST /messages/{id}/move`).

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

/// Where each listed message lives: id -> (account id, folder id).
pub(crate) fn message_location_index(state: &NativeAppState) -> BTreeMap<String, (String, String)> {
    state
        .messages
        .iter()
        .map(|m| (m.id.clone(), (m.account_id.clone(), m.folder_id.clone())))
        .collect()
}

/// Whether `folder` may receive `payload`: every message is known, belongs to
/// the folder's account, and at least one is not already there.
pub(crate) fn folder_drop_accepts(
    payload: &MessageDragPayload,
    folder: &Folder,
    locations: &BTreeMap<String, (String, String)>,
) -> bool {
    if payload.message_ids.is_empty() {
        return false;
    }
    let mut moves = false;
    for id in &payload.message_ids {
        match locations.get(id) {
            Some((account, current)) if *account == folder.account_id => {
                moves |= *current != folder.id;
            }
            _ => return false,
        }
    }
    moves
}

/// Queue `payload` for `folder_id`.  Returns `false` when the drop is rejected.
pub(crate) fn start_folder_drop(
    state_entity: &Entity<NativeAppState>,
    payload: &MessageDragPayload,
    folder_id: &str,
    app_cx: &mut App,
) -> bool {
    let accepted = state_entity.update(app_cx, |state, cx| {
        let locations = message_location_index(state);
        let Some(folder) = state
            .folders_by_account
            .values()
            .flatten()
            .find(|folder| folder.id == folder_id)
        else {
            return false;
        };
        if !folder_drop_accepts(payload, folder, &locations) {
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
        NativeMessageMutation::MoveToFolder {
            message_ids: payload.message_ids.clone(),
            folder_id: folder_id.to_string(),
        },
        app_cx,
    );
    true
}

/// Keyboard path: move the selection (or the focused row) to `folder_id`.
pub(crate) fn start_folder_move_for_selection(
    state_entity: &Entity<NativeAppState>,
    focused_id: Option<&str>,
    folder_id: &str,
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
    start_folder_drop(state_entity, &payload, folder_id, app_cx)
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
    let (labels, folders) = {
        let state = state_entity.read(app_cx);
        // Folders of the focused message's account only: a move never crosses accounts.
        let account = focused_id
            .as_deref()
            .and_then(|id| state.messages.iter().find(|m| m.id == id))
            .map(|m| m.account_id.clone());
        let folders: Vec<Folder> = state
            .folders_by_account
            .iter()
            .filter(|(account_id, _)| account.as_deref() == Some(account_id.as_str()))
            .flat_map(|(_, folders)| folders.iter().filter(|f| !f.is_excluded).cloned())
            .collect();
        (state.labels.clone(), folders)
    };
    let state_entity = state_entity.clone();
    app_cx.new(move |_| {
        let mut menu = ContextMenu::build(position);
        for folder in folders {
            let state_entity = state_entity.clone();
            let focused_id = focused_id.clone();
            let folder_id = folder.id.clone();
            menu = menu.action(
                format!("Folder: {}", folder_label(&folder)),
                move |_, _, app_cx| {
                    start_folder_move_for_selection(
                        &state_entity,
                        focused_id.as_deref(),
                        &folder_id,
                        app_cx,
                    );
                },
            );
        }
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

    fn folder(id: &str, account: &str) -> Folder {
        Folder {
            id: id.to_string(),
            account_id: account.to_string(),
            name: id.to_string(),
            full_path: id.to_string(),
            special_use: None,
            unread_count: 0,
            total_count: 0,
            synced_at: None,
            is_excluded: false,
        }
    }

    fn folder_state(selected: &[&str]) -> NativeAppState {
        let mut state = state(selected);
        state.folders_by_account.insert(
            "account".to_string(),
            vec![folder("inbox", "account"), folder("projects", "account")],
        );
        state
            .folders_by_account
            .insert("other".to_string(), vec![folder("elsewhere", "other")]);
        state
    }

    #[test]
    fn folder_drop_accepts_same_account_other_folder_only() {
        let locations = message_location_index(&folder_state(&[]));
        let p = payload(&["m1"]);
        assert!(folder_drop_accepts(
            &p,
            &folder("projects", "account"),
            &locations
        ));
        assert!(!folder_drop_accepts(
            &p,
            &folder("inbox", "account"),
            &locations
        ));
        assert!(!folder_drop_accepts(
            &p,
            &folder("elsewhere", "other"),
            &locations
        ));
        assert!(!folder_drop_accepts(
            &payload(&["gone"]),
            &folder("projects", "account"),
            &locations
        ));
        assert!(!folder_drop_accepts(
            &payload(&[]),
            &folder("projects", "account"),
            &locations
        ));
    }

    #[gpui::test]
    fn folder_drop_queues_one_undoable_move(cx: &mut gpui::TestAppContext) {
        fluent_test_utils::init_fluent_test(cx);
        let entity = cx.new(|_| folder_state(&["m1", "m3"]));
        let accepted =
            cx.update(|app| start_folder_drop(&entity, &payload(&["m1", "m3"]), "projects", app));
        assert!(accepted);
        entity.update(cx, |state, _| {
            assert!(state.selected_message_ids.is_empty());
            assert_eq!(
                state.pending_undo.as_ref().map(|p| p.mutation.clone()),
                Some(NativeMessageMutation::MoveToFolder {
                    message_ids: vec!["m1".to_string(), "m3".to_string()],
                    folder_id: "projects".to_string(),
                })
            );
        });
        cx.update(|app| cancel_pending_undo(&entity, app));
        entity.update(cx, |state, _| assert!(state.pending_undo.is_none()));
    }

    #[gpui::test]
    fn rejected_folder_drop_changes_nothing(cx: &mut gpui::TestAppContext) {
        fluent_test_utils::init_fluent_test(cx);
        let entity = cx.new(|_| folder_state(&["m1"]));
        for target in ["inbox", "elsewhere", "missing"] {
            assert!(!cx.update(|app| start_folder_drop(&entity, &payload(&["m1"]), target, app)));
        }
        entity.update(cx, |state, _| {
            assert!(state.pending_undo.is_none());
            assert_eq!(state.selected_message_ids, vec!["m1".to_string()]);
        });
    }

    #[gpui::test]
    fn keyboard_folder_move_uses_focused_row(cx: &mut gpui::TestAppContext) {
        fluent_test_utils::init_fluent_test(cx);
        let entity = cx.new(|_| folder_state(&[]));
        assert!(cx.update(|app| start_folder_move_for_selection(
            &entity,
            Some("m2"),
            "projects",
            app
        )));
        entity.update(cx, |state, _| {
            assert_eq!(
                state.pending_undo.as_ref().map(|p| p.mutation.clone()),
                Some(NativeMessageMutation::MoveToFolder {
                    message_ids: vec!["m2".to_string()],
                    folder_id: "projects".to_string(),
                })
            );
        });
    }
}
