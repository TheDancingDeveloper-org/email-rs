use email_types::{Account, Folder, FullMessage, Message};
use fluent_test_utils::{open_fluent_test_window, TestAppContext};
use gpui::AppContext as _;

// ---------------------------------------------------------------------------
// Test data helpers
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
        total_count: (unread.max(0)) + 5,
        synced_at: None,
        is_excluded: false,
    }
}

fn test_message(id: &str, is_read: bool) -> Message {
    Message {
        id: id.to_string(),
        account_id: "account-1".to_string(),
        folder_id: "inbox".to_string(),
        uid: 1,
        message_id: None,
        thread_id: None,
        subject: Some(format!("Test subject for {id}")),
        from_name: Some("Alice".to_string()),
        from_email: Some("alice@example.test".to_string()),
        to_json: None,
        date: Some("2026-05-28T12:00:00Z".to_string()),
        is_read,
        is_flagged: false,
        is_draft: false,
        has_attachments: false,
        preview: Some("Email preview text...".to_string()),
        snoozed_until: None,
    }
}

fn test_full_message(id: &str) -> FullMessage {
    FullMessage {
        id: id.to_string(),
        account_id: "account-1".to_string(),
        folder_id: "inbox".to_string(),
        uid: 1,
        message_id: None,
        thread_id: None,
        subject: Some("Full message subject".to_string()),
        from_name: Some("Alice".to_string()),
        from_email: Some("alice@example.test".to_string()),
        to: None,
        date: Some("2026-05-28T12:00:00Z".to_string()),
        is_read: true,
        is_flagged: false,
        is_draft: false,
        has_attachments: false,
        preview: Some("Preview...".to_string()),
        body: Some(email_types::MessageBody {
            html_body: None,
            text_body: Some("Plain-text body of the email.".to_string()),
        }),
        attachments: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// GPUI context sanity check
// ---------------------------------------------------------------------------

/// Verify that the Fluent theme context initialises correctly in a test run.
#[gpui::test]
fn fluent_theme_context_initialises(cx: &mut TestAppContext) {
    fluent_test_utils::init_fluent_test(cx);

    cx.update(|cx| {
        use fluent_core::ThemeProvider as _;
        let _ = cx.theme().colors.accent;
    });
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

/// SidebarPlaceholder renders without panic when the account list is empty.
#[gpui::test]
fn sidebar_renders_empty(cx: &mut TestAppContext) {
    let (_view, vcx) = open_fluent_test_window(cx, |_window, cx| {
        let state = cx.new(|_| crate::NativeAppState::default());
        crate::SidebarPlaceholder::new(cx, state)
    });
    vcx.run_until_parked();
}

/// SidebarPlaceholder renders without panic when loaded with accounts and folders.
#[gpui::test]
fn sidebar_renders_with_accounts_and_folders(cx: &mut TestAppContext) {
    let (_view, vcx) = open_fluent_test_window(cx, |_window, cx| {
        let mut state = crate::NativeAppState::default();
        state.accounts = vec![test_account("account-1"), test_account("account-2")];
        state.folders_by_account.insert(
            "account-1".to_string(),
            vec![test_folder("Inbox", 3), test_folder("Sent", 0)],
        );
        state
            .folders_by_account
            .insert("account-2".to_string(), vec![test_folder("Inbox", 0)]);
        state.selected_folder = Some("Inbox".to_string());
        let state = cx.new(|_| state);
        crate::SidebarPlaceholder::new(cx, state)
    });
    vcx.run_until_parked();
}

// ---------------------------------------------------------------------------
// Message list
// ---------------------------------------------------------------------------

/// MessageListPlaceholder renders without panic when there are no messages.
#[gpui::test]
fn message_list_renders_empty(cx: &mut TestAppContext) {
    let (_view, vcx) = open_fluent_test_window(cx, |_window, cx| {
        let state = cx.new(|_| crate::NativeAppState::default());
        crate::MessageListPlaceholder::new(cx, state)
    });
    vcx.run_until_parked();
}

/// MessageListPlaceholder renders without panic with a populated message list.
#[gpui::test]
fn message_list_renders_with_messages(cx: &mut TestAppContext) {
    let (_view, vcx) = open_fluent_test_window(cx, |_window, cx| {
        let mut state = crate::NativeAppState::default();
        state.accounts = vec![test_account("account-1")];
        state
            .folders_by_account
            .insert("account-1".to_string(), vec![test_folder("inbox", 2)]);
        state.messages = vec![
            test_message("msg-1", false),
            test_message("msg-2", true),
            test_message("msg-3", false),
        ];
        state.selected_folder = Some("inbox".to_string());
        state.selected_message = Some("msg-1".to_string());
        let state = cx.new(|_| state);
        crate::MessageListPlaceholder::new(cx, state)
    });
    vcx.run_until_parked();
}

// ---------------------------------------------------------------------------
// Reading pane
// ---------------------------------------------------------------------------

/// ReadingPanePlaceholder renders without panic when no message is selected.
#[gpui::test]
fn reading_pane_renders_empty(cx: &mut TestAppContext) {
    let (_view, vcx) = open_fluent_test_window(cx, |_window, cx| {
        let state = cx.new(|_| crate::NativeAppState::default());
        crate::ReadingPanePlaceholder::new(cx, state)
    });
    vcx.run_until_parked();
}

/// ReadingPanePlaceholder renders without panic when a full message is loaded.
#[gpui::test]
fn reading_pane_renders_with_full_message(cx: &mut TestAppContext) {
    let (_view, vcx) = open_fluent_test_window(cx, |_window, cx| {
        let mut state = crate::NativeAppState::default();
        state.accounts = vec![test_account("account-1")];
        state.messages = vec![test_message("msg-1", true)];
        state.selected_message = Some("msg-1".to_string());
        state.selected_full_message = Some(test_full_message("msg-1"));
        let state = cx.new(|_| state);
        crate::ReadingPanePlaceholder::new(cx, state)
    });
    vcx.run_until_parked();
}

// ---------------------------------------------------------------------------
// Calendar
// ---------------------------------------------------------------------------

/// CalendarPlaceholder renders without panic when there are no events.
#[gpui::test]
fn calendar_renders_empty(cx: &mut TestAppContext) {
    let (_view, vcx) = open_fluent_test_window(cx, |_window, cx| {
        let state = cx.new(|_| crate::NativeAppState::default());
        crate::CalendarPlaceholder::new(cx, state)
    });
    vcx.run_until_parked();
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

/// StatusBar renders without panic with a default (empty) application state.
#[gpui::test]
fn status_bar_renders_empty(cx: &mut TestAppContext) {
    let (_view, vcx) = open_fluent_test_window(cx, |_window, cx| {
        let state = cx.new(|_| crate::NativeAppState::default());
        crate::StatusBar::new(cx, state)
    });
    vcx.run_until_parked();
}

/// StatusBar renders without panic when there are unread messages.
#[gpui::test]
fn status_bar_renders_with_unread(cx: &mut TestAppContext) {
    let (_view, vcx) = open_fluent_test_window(cx, |_window, cx| {
        let mut state = crate::NativeAppState::default();
        state.accounts = vec![test_account("account-1")];
        state
            .folders_by_account
            .insert("account-1".to_string(), vec![test_folder("Inbox", 5)]);
        let state = cx.new(|_| state);
        crate::StatusBar::new(cx, state)
    });
    vcx.run_until_parked();
}
