//! The sign-in screen and shell, driven through egui_kittest with a fake
//! auth backend.

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use email_ui::auth::{AuthMode, Credentials};
use email_ui::{AuthApi, AuthError, EmailApp, Exec, MemoryStore, Screen, SignedIn};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct FakeAuth {
    calls: Mutex<Vec<String>>,
    restore: Option<Result<String, AuthError>>,
}

impl FakeAuth {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl AuthApi for FakeAuth {
    fn sign_in(&self, c: &Credentials) -> Result<SignedIn, AuthError> {
        let kind = match c.mode {
            AuthMode::SignIn => "login",
            AuthMode::FirstRun => "bootstrap",
        };
        self.calls
            .lock()
            .unwrap()
            .push(format!("{kind} {}", c.username));
        if c.password == "right password" {
            Ok(SignedIn {
                token: "tok-1".into(),
                user_id: format!("user-{}", c.username),
            })
        } else {
            Err(AuthError::Unauthorized(
                "invalid username or password".into(),
            ))
        }
    }

    fn restore(&self, token: &str) -> Result<String, AuthError> {
        self.calls.lock().unwrap().push(format!("me {token}"));
        self.restore.clone().unwrap_or(Ok("user-restored".into()))
    }

    fn sign_out(&self, token: &str) -> Result<(), AuthError> {
        self.calls.lock().unwrap().push(format!("logout {token}"));
        Ok(())
    }
}

fn harness(app: EmailApp) -> Harness<'static, EmailApp> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(900.0, 640.0))
        .build_ui_state(|ui, app: &mut EmailApp| app.ui(ui), app);
    EmailApp::install_theme(&harness.ctx);
    harness.run();
    harness
}

fn type_into(harness: &mut Harness<'static, EmailApp>, label: &str, text: &str) {
    harness.get_by_label(label).click();
    harness.run();
    harness.get_by_label(label).type_text(text);
    harness.run();
}

#[test]
fn signs_in_saves_the_token_and_shows_the_shell() {
    let api = Arc::new(FakeAuth::default());
    let store = MemoryStore::default();
    let mut h = harness(EmailApp::new(
        api.clone(),
        Box::new(store.clone()),
        Exec::Inline,
    ));
    assert!(h.query_by_label("Sign in to email-rs").is_some());

    type_into(&mut h, "Username", "admin");
    type_into(&mut h, "Password", "right password");
    h.get_by_label("Sign in").click();
    h.run();

    assert_eq!(api.calls(), vec!["login admin"]);
    assert_eq!(store.token().as_deref(), Some("tok-1"));
    assert!(matches!(h.state().screen(), Screen::SignedIn(s) if s.user_id == "user-admin"));
    assert!(h.query_by_label("Signed in as user-admin").is_some());
    assert!(h.query_by_label("Folders").is_some());
}

#[test]
fn wrong_password_shows_an_error_and_stores_nothing() {
    let api = Arc::new(FakeAuth::default());
    let store = MemoryStore::default();
    let mut h = harness(EmailApp::new(
        api.clone(),
        Box::new(store.clone()),
        Exec::Inline,
    ));

    type_into(&mut h, "Username", "admin");
    type_into(&mut h, "Password", "wrong");
    h.get_by_label("Sign in").click();
    h.run();

    assert_eq!(h.state().error(), Some("Wrong username or password."));
    assert_eq!(*h.state().screen(), Screen::SignIn);
    assert_eq!(store.token(), None);
}

#[test]
fn empty_form_is_rejected_without_a_request() {
    let api = Arc::new(FakeAuth::default());
    let mut h = harness(EmailApp::new(
        api.clone(),
        Box::new(MemoryStore::default()),
        Exec::Inline,
    ));
    h.get_by_label("Sign in").click();
    h.run();
    assert!(api.calls().is_empty());
    assert_eq!(
        h.state().error(),
        Some("Username and password are required.")
    );
}

#[test]
fn first_run_calls_bootstrap() {
    let api = Arc::new(FakeAuth::default());
    let mut h = harness(EmailApp::new(
        api.clone(),
        Box::new(MemoryStore::default()),
        Exec::Inline,
    ));
    h.get_by_label("First run? Set up the first account")
        .click();
    h.run();
    assert!(h.query_by_label("Set up email-rs").is_some());

    type_into(&mut h, "Username", "owner");
    type_into(&mut h, "Password", "right password");
    h.get_by_label("Create account").click();
    h.run();

    assert_eq!(api.calls(), vec!["bootstrap owner"]);
    assert!(matches!(h.state().screen(), Screen::SignedIn(_)));
}

#[test]
fn stored_token_restores_the_session() {
    let api = Arc::new(FakeAuth::default());
    let store = MemoryStore::with_token("saved");
    let h = harness(EmailApp::new(api.clone(), Box::new(store), Exec::Inline));
    assert_eq!(api.calls(), vec!["me saved"]);
    assert!(matches!(
        h.state().screen(),
        Screen::SignedIn(s) if s.token == "saved" && s.user_id == "user-restored"
    ));
}

#[test]
fn expired_token_is_dropped() {
    let api = Arc::new(FakeAuth {
        restore: Some(Err(AuthError::Unauthorized("session expired".into()))),
        ..Default::default()
    });
    let store = MemoryStore::with_token("old");
    let h = harness(EmailApp::new(api, Box::new(store.clone()), Exec::Inline));
    assert_eq!(*h.state().screen(), Screen::SignIn);
    assert_eq!(store.token(), None);
    assert_eq!(
        h.state().error(),
        Some("Your session has ended. Sign in again.")
    );
}

#[test]
fn unreachable_server_keeps_the_token() {
    let api = Arc::new(FakeAuth {
        restore: Some(Err(AuthError::Failed("could not reach the server".into()))),
        ..Default::default()
    });
    let store = MemoryStore::with_token("kept");
    let h = harness(EmailApp::new(api, Box::new(store.clone()), Exec::Inline));
    assert_eq!(*h.state().screen(), Screen::SignIn);
    assert_eq!(store.token().as_deref(), Some("kept"));
}

#[test]
fn sign_out_clears_the_token_and_calls_logout() {
    let api = Arc::new(FakeAuth::default());
    let store = MemoryStore::with_token("tok");
    let mut h = harness(EmailApp::new(
        api.clone(),
        Box::new(store.clone()),
        Exec::Inline,
    ));
    h.get_by_label("Sign out").click();
    h.run();
    assert_eq!(api.calls(), vec!["me tok", "logout tok"]);
    assert_eq!(store.token(), None);
    assert_eq!(*h.state().screen(), Screen::SignIn);
}
