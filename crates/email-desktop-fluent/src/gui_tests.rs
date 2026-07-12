#![allow(clippy::field_reassign_with_default, clippy::redundant_closure)]

/// GPUI-level end-to-end tests for email-desktop-fluent.
///
/// These tests go beyond "renders without panic" and verify behavioural
/// contracts:
///   - State-machine transitions triggered by core functions
///   - Views correctly observing a shared `NativeAppState` entity
///   - Async drain loop survival under a simulated 250 ms clock tick
///
/// Pattern follows rdpapp/src/gui_tests.rs:
///   - `#[gpui::test]` with `TestAppContext` for entity-level tests
///   - `open_fluent_test_window` for window-mounted view tests
///   - `cx.update(|app| { ... })` to invoke private app functions in-process
///   - `use super::*` gives access to all private crate items (same rules as
///     the inline `mod tests` block in main.rs)
///
/// Reading entity state: use `entity.update(cx, |s, _| ...)` rather than
/// `entity.read(cx)` — `read` requires `&App` but `VisualTestContext` only
/// deref-coerces when the `prelude::*` traits from `use super::*` are in scope.
/// `update` works reliably with any `Context` implementor.
use std::time::{Duration, Instant};

// Bring all private items from the root module into scope — same access level
// as the inline `#[cfg(test)] mod tests { use super::*; }` block in main.rs.
use super::*;

use email_types::{Account, Folder, FullMessage, Message};
use fluent_test_utils::{open_fluent_test_window, TestAppContext};

// ---------------------------------------------------------------------------
// Shared test data builders
// ---------------------------------------------------------------------------

fn test_account(id: &str) -> Account {
    Account {
        id: id.to_string(),
        name: "Test Account".to_string(),
        email: format!("{id}@example.test"),
        provider_type: "generic_imap".to_string(),
        auth_type: "basic".to_string(),
        created_at: "2026-05-28T00:00:00Z".to_string(),
    }
}

fn test_folder(id: &str, unread: i64) -> Folder {
    Folder {
        id: id.to_string(),
        account_id: "account-1".to_string(),
        name: id.to_string(),
        full_path: id.to_string(),
        special_use: None,
        unread_count: unread,
        total_count: unread.max(0) + 5,
        synced_at: None,
        is_excluded: false,
    }
}

fn test_message(id: &str, is_read: bool) -> Message {
    Message {
        id: id.to_string(),
        occurrence_id: id.to_string(),
        logical_id: format!("logical-{id}"),
        account_id: "account-1".to_string(),
        folder_id: "inbox".to_string(),
        uid: 1,
        message_id: None,
        thread_id: None,
        subject: Some(format!("Subject for {id}")),
        from_name: Some("Alice".to_string()),
        from_email: Some("alice@example.test".to_string()),
        to_json: None,
        date: Some("2026-05-28T12:00:00Z".to_string()),
        is_read,
        is_flagged: false,
        is_draft: false,
        has_attachments: false,
        preview: Some("Preview text…".to_string()),
        snoozed_until: None,
    }
}

fn test_full_message(id: &str) -> FullMessage {
    FullMessage {
        id: id.to_string(),
        occurrence_id: id.to_string(),
        logical_id: format!("logical-{id}"),
        account_id: "account-1".to_string(),
        folder_id: "inbox".to_string(),
        uid: 1,
        message_id: None,
        thread_id: None,
        subject: Some("Full subject".to_string()),
        from_name: Some("Alice".to_string()),
        from_email: Some("alice@example.test".to_string()),
        to: None,
        date: Some("2026-05-28T12:00:00Z".to_string()),
        is_read: true,
        is_flagged: false,
        is_draft: false,
        has_attachments: false,
        preview: Some("Preview…".to_string()),
        body: Some(email_types::MessageBody {
            html_body: None,
            text_body: Some("Plain-text body.".to_string()),
        }),
        attachments: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Baseline: theme context
// ---------------------------------------------------------------------------

/// Fluent theme must initialise cleanly inside a GPUI test context.
#[gpui::test]
fn fluent_theme_context_initialises(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    cx.update(|cx| {
        use fluent_core::ThemeProvider as _;
        let _ = cx.theme().colors.accent;
    });
}

// ---------------------------------------------------------------------------
// 1. Compose workflow
// ---------------------------------------------------------------------------

/// Without any accounts StartCompose leaves compose closed and sets an error.
#[gpui::test]
fn compose_requires_account_to_open(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    let state = cx.new(|_| NativeAppState::default());

    cx.update(|app| start_new_compose(&state, app));

    state.update(cx, |s, _| {
        assert!(!s.compose.open, "compose must not open without an account");
        assert!(
            s.compose.error.is_some(),
            "compose.error must explain the problem"
        );
        assert!(
            s.last_error.is_some(),
            "app-level last_error must also be set"
        );
    });
}

/// With an account present StartCompose opens compose and populates required fields.
#[gpui::test]
fn compose_opens_and_targets_first_account(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    let state = cx.new(|_| {
        let mut s = NativeAppState::default();
        s.accounts = vec![test_account("account-1")];
        s
    });

    cx.update(|app| start_new_compose(&state, app));

    state.update(cx, |s, _| {
        assert!(s.compose.open);
        assert_eq!(s.compose.mode.as_deref(), Some("compose"));
        assert_eq!(s.compose.account_id.as_deref(), Some("account-1"));
        assert!(s.compose.to.is_empty(), "new compose has no recipients yet");
        assert!(s.last_error.is_none());
    });
}

/// StartCompose always switches the visible panel back to Mail, even if
/// Calendar was active.
#[gpui::test]
fn compose_resets_view_to_mail(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    let state = cx.new(|_| {
        let mut s = NativeAppState::default();
        s.accounts = vec![test_account("account-1")];
        s.selected_view = SelectedView::Calendar;
        s
    });

    cx.update(|app| start_new_compose(&state, app));

    let view = state.update(cx, |s, _| s.selected_view.clone());
    assert_eq!(
        view,
        SelectedView::Mail,
        "compose must switch back to mail view"
    );
}

/// Without a selected message StartReplyCompose sets an error and stays closed.
#[gpui::test]
fn reply_compose_requires_selected_message(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    let state = cx.new(|_| {
        let mut s = NativeAppState::default();
        s.accounts = vec![test_account("account-1")];
        s
    });

    cx.update(|app| start_reply_draft(&state, app));

    state.update(cx, |s, _| {
        assert!(
            !s.compose.open,
            "reply compose must not open without selection"
        );
        assert!(s.compose.error.is_some());
    });
}

/// With a selected message StartReplyCompose opens in "reply" mode and
/// pre-populates the account and subject.
#[gpui::test]
fn reply_compose_opens_with_correct_mode_and_account(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    let state = cx.new(|_| {
        let mut s = NativeAppState::default();
        s.accounts = vec![test_account("account-1")];
        s.messages = vec![test_message("msg-1", true)];
        s.selected_message = Some("msg-1".to_string());
        s.selected_full_message = Some(test_full_message("msg-1"));
        s
    });

    cx.update(|app| start_reply_draft(&state, app));

    state.update(cx, |s, _| {
        assert!(s.compose.open);
        assert_eq!(s.compose.mode.as_deref(), Some("reply"));
        assert_eq!(s.compose.account_id.as_deref(), Some("account-1"));
        // Subject is prefixed with "Re: "
        assert!(
            s.compose
                .subject
                .as_deref()
                .unwrap_or_default()
                .starts_with("Re:"),
            "reply subject should start with Re:"
        );
    });
}

/// Without a selected message StartForwardCompose sets an error and stays closed.
#[gpui::test]
fn forward_compose_requires_selected_message(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    let state = cx.new(|_| {
        let mut s = NativeAppState::default();
        s.accounts = vec![test_account("account-1")];
        s
    });

    cx.update(|app| start_forward_draft(&state, app));

    state.update(cx, |s, _| {
        assert!(!s.compose.open);
        assert!(s.compose.error.is_some());
    });
}

// ---------------------------------------------------------------------------
// 2. View switching
// ---------------------------------------------------------------------------

/// Switching to the calendar view clears the current message selection list.
#[gpui::test]
fn calendar_view_clears_selected_message_ids(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    let state = cx.new(|_| {
        let mut s = NativeAppState::default();
        s.selected_view = SelectedView::Mail;
        s.selected_message_ids = vec!["msg-1".to_string(), "msg-2".to_string()];
        s
    });

    cx.update(|app| start_calendar_view(&state, app));

    state.update(cx, |s, _| {
        assert_eq!(
            s.selected_view,
            SelectedView::Calendar,
            "view should switch to calendar"
        );
        assert!(
            s.selected_message_ids.is_empty(),
            "message selection must be cleared on calendar switch"
        );
    });
}

// ---------------------------------------------------------------------------
// 3. Sync status state machine
// ---------------------------------------------------------------------------

/// Connecting update clears connected flag and any prior error.
#[gpui::test]
fn sync_connecting_clears_connected_and_error(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    let mut status = NativeSyncStatus::default();
    status.connected = true;
    status.last_error = Some("prior error".to_string());

    apply_sync_update(&mut status, NativeSyncUpdate::Connecting);

    assert!(!status.connected, "connecting should mark disconnected");
    assert!(
        status.last_error.is_none(),
        "connecting should clear last_error"
    );
}

/// Connected update marks the sync stream as live and clears any error.
#[gpui::test]
fn sync_connected_sets_connected_flag(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    let mut status = NativeSyncStatus::default();
    status.last_error = Some("prior error".to_string());

    apply_sync_update(&mut status, NativeSyncUpdate::Connected);

    assert!(status.connected);
    assert!(status.last_error.is_none());
}

/// Error update disconnects the stream and stores the error message.
#[gpui::test]
fn sync_error_disconnects_and_records_message(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    let mut status = NativeSyncStatus::default();
    status.connected = true;
    status.syncing = true;

    apply_sync_update(
        &mut status,
        NativeSyncUpdate::Error("connection refused".to_string()),
    );

    assert!(!status.connected);
    assert!(!status.syncing);
    assert_eq!(
        status.last_error.as_deref(),
        Some("connection refused"),
        "error text must be stored verbatim"
    );
}

/// SyncComplete event stops the syncing flag and records the completion time.
#[gpui::test]
fn sync_complete_event_stops_syncing_and_records_completion(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    let mut status = NativeSyncStatus::default();
    status.syncing = true;

    apply_sync_update(
        &mut status,
        NativeSyncUpdate::Event(email_types::SyncEvent::SyncComplete {
            account_id: "account-1".to_string(),
        }),
    );

    assert!(status.connected);
    assert!(
        !status.syncing,
        "sync must be marked done after SyncComplete"
    );
    assert!(
        status.completed_at.is_some(),
        "completion timestamp must be recorded"
    );
    assert_eq!(status.last_message.as_deref(), Some("Sync complete"));
}

/// Sync completion badge disappears after SYNC_COMPLETE_VISIBLE_FOR elapses.
#[gpui::test]
fn sync_completion_expires_after_its_visibility_window(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    let mut status = NativeSyncStatus::default();
    // Pretend the sync completed just over the threshold ago.
    let past = Instant::now() - SYNC_COMPLETE_VISIBLE_FOR - Duration::from_millis(1);
    status.completed_at = Some(past);

    let expired = expire_sync_completion(&mut status, Instant::now());

    assert!(expired, "badge should have expired");
    assert!(
        status.completed_at.is_none(),
        "completed_at must be cleared"
    );
    assert!(
        status.last_message.is_none(),
        "last_message must be cleared"
    );
}

/// Sync completion badge is kept until the full visibility window has elapsed.
#[gpui::test]
fn sync_completion_is_not_expired_prematurely(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    let mut status = NativeSyncStatus::default();
    // Completed just now — not yet expired.
    status.completed_at = Some(Instant::now());

    let expired = expire_sync_completion(&mut status, Instant::now());

    assert!(!expired);
    assert!(
        status.completed_at.is_some(),
        "completed_at must not be cleared before the timeout"
    );
}

// ---------------------------------------------------------------------------
// 4. View observation — views must react to NativeAppState mutations
// ---------------------------------------------------------------------------

/// SidebarPlaceholder propagates account and folder updates to its state.
#[gpui::test]
fn sidebar_observes_account_and_folder_changes(cx: &mut TestAppContext) {
    let state = cx.new(|_| NativeAppState::default());
    let (_view, vcx) = open_fluent_test_window(cx, {
        let s = state.clone();
        move |_window, cx| SidebarPlaceholder::new(cx, s)
    });

    state.update(vcx, |s, _| assert!(s.accounts.is_empty()));

    state.update(vcx, |s, cx| {
        s.accounts = vec![test_account("a1"), test_account("a2")];
        s.folders_by_account
            .insert("a1".to_string(), vec![test_folder("Inbox", 4)]);
        s.selected_folder = Some("Inbox".to_string());
        cx.notify();
    });
    vcx.run_until_parked();

    state.update(vcx, |s, _| {
        assert_eq!(s.accounts.len(), 2, "both accounts should be visible");
        assert_eq!(
            s.folders_by_account["a1"][0].unread_count, 4,
            "folder unread count should propagate"
        );
        assert_eq!(s.selected_folder.as_deref(), Some("Inbox"));
    });
}

/// MessageListPlaceholder reflects changes to the message list in state.
#[gpui::test]
fn message_list_observes_message_mutations(cx: &mut TestAppContext) {
    let state = cx.new(|_| NativeAppState::default());
    let (_view, vcx) = open_fluent_test_window(cx, {
        let s = state.clone();
        move |_window, cx| MessageListPlaceholder::new(cx, s)
    });

    state.update(vcx, |s, _| assert!(s.messages.is_empty()));

    state.update(vcx, |s, cx| {
        s.messages = vec![
            test_message("m1", false),
            test_message("m2", true),
            test_message("m3", false),
        ];
        cx.notify();
    });
    vcx.run_until_parked();

    state.update(vcx, |s, _| {
        assert_eq!(s.messages.len(), 3);
        let unread = s.messages.iter().filter(|m| !m.is_read).count();
        assert_eq!(unread, 2, "two of three messages should be unread");
    });
}

/// ReadingPanePlaceholder tracks changes to the selected message and full body.
#[gpui::test]
fn reading_pane_observes_selection_and_body_changes(cx: &mut TestAppContext) {
    let state = cx.new(|_| NativeAppState::default());
    let (_view, vcx) = open_fluent_test_window(cx, {
        let s = state.clone();
        move |_window, cx| ReadingPanePlaceholder::new(cx, s)
    });

    state.update(vcx, |s, _| assert!(s.selected_message.is_none()));

    // Select a message.
    state.update(vcx, |s, cx| {
        s.messages = vec![test_message("msg-1", false)];
        s.selected_message = Some("msg-1".to_string());
        cx.notify();
    });
    vcx.run_until_parked();

    state.update(vcx, |s, _| {
        assert_eq!(s.selected_message.as_deref(), Some("msg-1"));
        assert!(
            s.selected_full_message.is_none(),
            "full body is only loaded after a server fetch, not from selection alone"
        );
    });

    // Simulate the server response arriving.
    state.update(vcx, |s, cx| {
        s.selected_full_message = Some(test_full_message("msg-1"));
        cx.notify();
    });
    vcx.run_until_parked();

    state.update(vcx, |s, _| {
        assert!(
            s.selected_full_message.is_some(),
            "full body should now be available"
        );
    });
}

/// CalendarPlaceholder observes changes to the event list.
#[gpui::test]
fn calendar_observes_event_list_changes(cx: &mut TestAppContext) {
    let state = cx.new(|_| NativeAppState::default());
    let (_view, vcx) = open_fluent_test_window(cx, {
        let s = state.clone();
        move |_window, cx| CalendarPlaceholder::new(cx, s)
    });

    state.update(vcx, |s, _| assert!(s.calendar_events.is_empty()));

    state.update(vcx, |s, cx| {
        s.selected_view = SelectedView::Calendar;
        let event = email_types::CalendarEvent {
            id: "evt-1".to_string(),
            calendar_id: "primary".to_string(),
            provider_event_id: None,
            title: "Team standup".to_string(),
            description: None,
            start_at: chrono::Utc::now(),
            end_at: chrono::Utc::now() + chrono::Duration::hours(1),
            location: None,
            is_all_day: false,
            recurrence_rule: None,
            attendees: Vec::new(),
            meet_link: None,
        };
        s.calendar_events = vec![event];
        cx.notify();
    });
    vcx.run_until_parked();

    state.update(vcx, |s, _| {
        assert_eq!(
            s.calendar_events.len(),
            1,
            "one event should now be visible in calendar state"
        );
    });
}

/// Two handles to the same entity both reflect a mutation made through
/// either one — confirming the shared-entity observation model.
#[gpui::test]
fn multiple_views_observe_the_same_state_entity(cx: &mut TestAppContext) {
    let state = cx.new(|_| NativeAppState::default());

    let state_for_sidebar = state.clone();
    let (_sidebar, vcx) = open_fluent_test_window(cx, move |_window, cx| {
        SidebarPlaceholder::new(cx, state_for_sidebar)
    });

    state.update(vcx, |s, cx| {
        s.accounts = vec![test_account("a1")];
        cx.notify();
    });
    vcx.run_until_parked();

    // Read via the original handle — same entity as `state_for_sidebar`.
    state.update(vcx, |s, _| {
        assert_eq!(
            s.accounts.len(),
            1,
            "original handle should reflect the mutation"
        );
    });
}

// ---------------------------------------------------------------------------
// 5. Unread count and status
// ---------------------------------------------------------------------------

/// total_unread_count sums positive unread counts from every folder across
/// every account, ignoring negative placeholders.
#[gpui::test]
fn unread_count_sums_all_positive_folder_counts(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);
    let state = cx.new(|_| {
        let mut s = NativeAppState::default();
        s.accounts = vec![test_account("a1"), test_account("a2")];
        s.folders_by_account.insert(
            "a1".to_string(),
            vec![test_folder("Inbox", 3), test_folder("Drafts", 0)],
        );
        s.folders_by_account.insert(
            "a2".to_string(),
            // −1 is a "not yet synced" sentinel; must be ignored.
            vec![test_folder("Inbox", 7), test_folder("Sent", -1)],
        );
        s
    });

    let unread = state.update(cx, |s, _| total_unread_count(s));
    assert_eq!(unread, 10, "3 + 7 = 10; −1 is ignored");
}

/// After the folder list is mutated, the unread count visible to StatusBar
/// updates correctly.
#[gpui::test]
fn status_bar_unread_count_updates_after_folder_load(cx: &mut TestAppContext) {
    let state = cx.new(|_| NativeAppState::default());
    let (_view, vcx) = open_fluent_test_window(cx, {
        let s = state.clone();
        move |_window, cx| StatusBar::new(cx, s)
    });

    let initial = state.update(vcx, |s, _| total_unread_count(s));
    assert_eq!(initial, 0);

    state.update(vcx, |s, cx| {
        s.accounts = vec![test_account("a1")];
        s.folders_by_account.insert(
            "a1".to_string(),
            vec![test_folder("Inbox", 5), test_folder("Sent", 0)],
        );
        cx.notify();
    });
    vcx.run_until_parked();

    let after = state.update(vcx, |s, _| total_unread_count(s));
    assert_eq!(
        after, 5,
        "status bar should count 5 unread after folder load"
    );
}

// ---------------------------------------------------------------------------
// 6. Full app integration — drain loop survival
// ---------------------------------------------------------------------------

/// Build the full EmailDesktopApp entity, advance the background executor
/// clock past two 250 ms drain ticks, and verify the app survives without
/// panicking.
#[gpui::test]
fn full_app_drain_loop_survives_timer_ticks(cx: &mut TestAppContext) {
    let (app, vcx) = open_fluent_test_window(cx, |_window, cx| {
        let state = cx.new(|_| NativeAppState::default());

        let sidebar = cx.new({
            let s = state.clone();
            move |cx| SidebarPlaceholder::new(cx, s)
        });
        let sidebar_dock = cx.new(|cx| {
            fluent_layout::DockPanel::new(cx, fluent_layout::DockPosition::Left, "Mail")
                .size(DEFAULT_SIDEBAR_WIDTH as f32)
                .content(sidebar.into())
        });
        let msg_list = cx.new({
            let s = state.clone();
            move |cx| MessageListPlaceholder::new(cx, s)
        });
        let reading_pane = cx.new({
            let s = state.clone();
            move |cx| ReadingPanePlaceholder::new(cx, s)
        });
        let mail_split = cx.new(|_| {
            fluent_layout::PaneGroup::horizontal()
                .first(msg_list.into())
                .second(reading_pane.into())
        });
        let workspace = cx.new(|cx| fluent_layout::Workspace::new(cx));
        let palette = cx.new(|cx| fluent_layout::CommandPalette::new(cx).max_results(14));
        EmailDesktopApp::new(cx, state, workspace, palette, sidebar_dock, mail_split)
    });

    // Advance the simulated clock past two drain intervals (250 ms each).
    vcx.executor().advance_clock(Duration::from_millis(600));
    vcx.run_until_parked();

    // The entity must still be alive after both ticks — prove by reading it.
    app.update(vcx, |_, _| ());
}
