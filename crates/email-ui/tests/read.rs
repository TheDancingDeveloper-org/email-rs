//! The read path: folders, the message list and the reading pane, driven
//! through egui_kittest with fake auth and mail backends.

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use email_types::{Account, Folder, FullMessage, Message, MessageBody};
use email_ui::app::Load;
use email_ui::auth::Credentials;
use email_ui::{
    AuthApi, AuthError, EmailApp, Exec, MailApi, Mailbox, MemoryStore, Screen, SignedIn,
};
use std::sync::{Arc, Mutex};

struct FakeAuth;

impl AuthApi for FakeAuth {
    fn sign_in(&self, _: &Credentials) -> Result<SignedIn, AuthError> {
        unreachable!("tests start from a stored token")
    }
    fn restore(&self, _: &str) -> Result<String, AuthError> {
        Ok("user-1".into())
    }
    fn sign_out(&self, _: &str) -> Result<(), AuthError> {
        Ok(())
    }
}

#[derive(Default)]
struct FakeMail {
    calls: Mutex<Vec<String>>,
    /// Every call fails with this when set.
    fail: Option<AuthError>,
}

impl FakeMail {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
    fn record(&self, call: String) -> Result<(), AuthError> {
        self.calls.lock().unwrap().push(call);
        self.fail.clone().map_or(Ok(()), Err)
    }
}

fn folder(id: &str, name: &str, unread: i64) -> Folder {
    Folder {
        id: id.into(),
        account_id: "acc-1".into(),
        name: name.into(),
        full_path: name.into(),
        special_use: None,
        unread_count: unread,
        total_count: 2,
        synced_at: None,
        is_excluded: false,
    }
}

fn message(id: &str, from: &str, subject: &str) -> Message {
    Message {
        id: id.into(),
        occurrence_id: id.into(),
        logical_id: id.into(),
        account_id: "acc-1".into(),
        folder_id: "inbox".into(),
        uid: 1,
        message_id: None,
        thread_id: None,
        subject: Some(subject.into()),
        from_name: Some(from.into()),
        from_email: Some("ann@example.test".into()),
        to_json: None,
        date: Some("2026-10-30T09:00:00Z".into()),
        is_read: false,
        is_flagged: false,
        is_draft: false,
        has_attachments: false,
        preview: None,
        snoozed_until: None,
    }
}

impl MailApi for FakeMail {
    fn mailboxes(&self, token: &str) -> Result<Vec<Mailbox>, AuthError> {
        self.record(format!("mailboxes {token}"))?;
        Ok(vec![Mailbox {
            account: Account {
                id: "acc-1".into(),
                name: "Work".into(),
                email: "me@example.test".into(),
                provider_type: "imap".into(),
                auth_type: "password".into(),
                created_at: "2026-10-01".into(),
            },
            folders: vec![
                folder("inbox", "Inbox", 2),
                folder("archive", "Archive", 0),
                Folder {
                    is_excluded: true,
                    ..folder("spam", "Junk", 0)
                },
            ],
        }])
    }

    fn messages(&self, token: &str, folder_id: &str) -> Result<Vec<Message>, AuthError> {
        self.record(format!("messages {token} {folder_id}"))?;
        Ok(match folder_id {
            "inbox" => vec![
                message("m1", "Ann", "Quarterly report"),
                message("m2", "Bob", "Lunch"),
            ],
            _ => vec![],
        })
    }

    fn message(&self, token: &str, message_id: &str) -> Result<FullMessage, AuthError> {
        self.record(format!("message {token} {message_id}"))?;
        let m = message(message_id, "Ann", "Quarterly report");
        Ok(FullMessage {
            id: m.id.clone(),
            occurrence_id: m.id.clone(),
            logical_id: m.id,
            account_id: m.account_id,
            folder_id: m.folder_id,
            uid: 1,
            message_id: None,
            thread_id: None,
            subject: m.subject,
            from_name: m.from_name,
            from_email: m.from_email,
            to: Some("me@example.test".into()),
            date: m.date,
            is_read: true,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: None,
            body: Some(MessageBody {
                html_body: Some("<p>Numbers are <b>up</b> this quarter.</p>".into()),
                ..Default::default()
            }),
            attachments: vec![],
        })
    }
}

fn harness(mail: Arc<FakeMail>, store: MemoryStore) -> Harness<'static, EmailApp> {
    let app = EmailApp::new(Arc::new(FakeAuth), Box::new(store), Exec::Inline).with_mail(mail);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 700.0))
        .build_ui_state(|ui, app: &mut EmailApp| app.ui(ui), app);
    EmailApp::install_theme(&harness.ctx);
    harness.run();
    harness
}

#[test]
fn lists_folders_then_messages_then_opens_one() {
    let mail = Arc::new(FakeMail::default());
    let mut h = harness(mail.clone(), MemoryStore::with_token("tok"));

    assert_eq!(mail.calls(), vec!["mailboxes tok"]);
    assert!(h.query_by_label("me@example.test").is_some());
    assert!(h.query_by_label("Inbox (2)").is_some());
    assert!(h.query_by_label("Archive").is_some());
    assert!(
        h.query_by_label("Junk").is_none(),
        "excluded folders are hidden"
    );
    assert!(h.query_by_label("No folder selected").is_some());

    h.get_by_label("Inbox (2)").click();
    h.run();
    assert_eq!(mail.calls()[1], "messages tok inbox");
    assert!(h.query_by_label("Ann\nQuarterly report").is_some());
    assert!(h.query_by_label("Bob\nLunch").is_some());
    assert!(h.query_by_label("No message selected").is_some());

    h.get_by_label("Ann\nQuarterly report").click();
    h.run();
    assert_eq!(mail.calls()[2], "message tok m1");
    assert!(matches!(h.state().reader().message, Load::Ready(_)));
    assert!(h.query_by_label("Quarterly report").is_some());
    assert!(h.query_by_label("From: Ann <ann@example.test>").is_some());
    assert!(h.query_by_label("To: me@example.test").is_some());
    let Load::Ready(opened) = &h.state().reader().message else {
        unreachable!()
    };
    assert_eq!(
        opened.doc.to_plain_text().trim(),
        "Numbers are up this quarter."
    );
}

#[test]
fn switching_folder_closes_the_open_message() {
    let mail = Arc::new(FakeMail::default());
    let mut h = harness(mail.clone(), MemoryStore::with_token("tok"));
    h.get_by_label("Inbox (2)").click();
    h.run();
    h.get_by_label("Bob\nLunch").click();
    h.run();
    h.get_by_label("Archive").click();
    h.run();
    assert_eq!(h.state().reader().open, None);
    assert!(h.query_by_label("This folder is empty").is_some());
    assert!(h.query_by_label("No message selected").is_some());
}

#[test]
fn a_401_on_a_read_ends_the_session() {
    let mail = Arc::new(FakeMail {
        fail: Some(AuthError::Unauthorized("session expired".into())),
        ..Default::default()
    });
    let store = MemoryStore::with_token("old");
    let h = harness(mail, store.clone());
    assert_eq!(*h.state().screen(), Screen::SignIn);
    assert_eq!(store.token(), None);
    assert_eq!(
        h.state().error(),
        Some("Your session has ended. Sign in again.")
    );
}

#[test]
fn a_failed_read_shows_the_error_and_keeps_the_session() {
    let mail = Arc::new(FakeMail {
        fail: Some(AuthError::Failed("server busy".into())),
        ..Default::default()
    });
    let store = MemoryStore::with_token("tok");
    let h = harness(mail, store.clone());
    assert!(matches!(h.state().screen(), Screen::SignedIn(_)));
    assert_eq!(store.token().as_deref(), Some("tok"));
    assert!(h
        .query_by_label("Could not load folders: server busy")
        .is_some());
}

#[test]
fn sign_out_clears_the_reader() {
    let mail = Arc::new(FakeMail::default());
    let mut h = harness(mail, MemoryStore::with_token("tok"));
    h.get_by_label("Inbox (2)").click();
    h.run();
    h.get_by_label("Sign out").click();
    h.run();
    assert_eq!(h.state().reader().folder, None);
    assert_eq!(h.state().reader().mailboxes, Load::Idle);
}
