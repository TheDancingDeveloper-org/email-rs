//! The eframe app: restore a stored session, sign in, and the signed-in
//! shell with folders, the message list and the reading pane.

use crate::auth::{AuthMode, Credentials, LoginForm};
use crate::backend::{AuthApi, AuthError, SignedIn};
use crate::mail::{self, MailApi, Mailbox};
use crate::session::TokenStore;
use email_types::{FullMessage, Message};
use fluent2_egui::controls::{Appearance, Button, Field, Input, ValidationState};
use fluent2_egui::data::html::{HtmlDocument, HtmlView, LinkTarget};
use fluent2_egui::Theme;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// How auth calls run. The app uses a worker thread per call; tests run
/// them inline so a frame sees the result at once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Exec {
    Thread,
    Inline,
}

/// What the window shows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Screen {
    /// Checking a stored token with `GET /app-auth/me`.
    Restoring,
    SignIn,
    SignedIn(SignedIn),
}

enum Event {
    Restored(String, Result<String, AuthError>),
    SignedIn(Result<SignedIn, AuthError>),
    Mailboxes(Result<Vec<Mailbox>, AuthError>),
    /// A folder id and its messages.
    Messages(String, Result<Vec<Message>, AuthError>),
    /// A message id and the full message.
    Opened(String, Result<Box<FullMessage>, AuthError>),
}

/// A read call's state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Load<T> {
    #[default]
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

/// An opened message and its parsed body.
pub struct Opened {
    pub message: FullMessage,
    pub doc: HtmlDocument,
}

/// The read path's state. Reset on every sign-in and sign-out.
#[derive(Default)]
pub struct Reader {
    pub mailboxes: Load<Vec<Mailbox>>,
    pub folder: Option<String>,
    pub messages: Load<Vec<Message>>,
    pub open: Option<String>,
    pub message: Load<Opened>,
}

pub struct EmailApp {
    api: Arc<dyn AuthApi>,
    mail: Option<Arc<dyn MailApi>>,
    reader: Reader,
    store: Box<dyn TokenStore>,
    exec: Exec,
    screen: Screen,
    form: LoginForm,
    busy: bool,
    error: Option<String>,
    /// A non-fatal note, such as a keyring that would not save.
    notice: Option<String>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
}

impl EmailApp {
    /// Starts on [`Screen::Restoring`] when the store has a token, else on
    /// the sign-in screen.
    pub fn new(api: Arc<dyn AuthApi>, store: Box<dyn TokenStore>, exec: Exec) -> Self {
        let (tx, rx) = channel();
        let mut app = Self {
            api,
            mail: None,
            reader: Reader::default(),
            store,
            exec,
            screen: Screen::SignIn,
            form: LoginForm::default(),
            busy: false,
            error: None,
            notice: None,
            tx,
            rx,
        };
        if let Some(token) = app.store.load() {
            app.screen = Screen::Restoring;
            let api = app.api.clone();
            app.run(None, move || {
                let result = api.restore(&token);
                Event::Restored(token, result)
            });
        }
        app
    }

    /// Sets the read API. Without one the shell shows no folders.
    pub fn with_mail(mut self, mail: Arc<dyn MailApi>) -> Self {
        self.mail = Some(mail);
        self
    }

    pub fn reader(&self) -> &Reader {
        &self.reader
    }

    /// Installs the Fluent theme. Call once from the eframe creation hook.
    pub fn install_theme(ctx: &egui::Context) {
        fluent2_egui::install(ctx, &Theme::light(fluent2_egui::tokens::brand::FLUENT));
    }

    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn form_mut(&mut self) -> &mut LoginForm {
        &mut self.form
    }

    fn run(&self, ctx: Option<&egui::Context>, job: impl FnOnce() -> Event + Send + 'static) {
        let tx = self.tx.clone();
        match self.exec {
            Exec::Inline => {
                let _ = tx.send(job());
            }
            Exec::Thread => {
                let ctx = ctx.cloned();
                std::thread::spawn(move || {
                    let _ = tx.send(job());
                    if let Some(ctx) = ctx {
                        ctx.request_repaint();
                    }
                });
            }
        }
    }

    fn token(&self) -> Option<String> {
        match &self.screen {
            Screen::SignedIn(session) => Some(session.token.clone()),
            _ => None,
        }
    }

    /// Shows the shell for `session` and starts loading its mailboxes.
    fn enter_shell(&mut self, session: SignedIn) {
        self.screen = Screen::SignedIn(session);
        self.reader = Reader::default();
        self.reload_mailboxes(None);
    }

    /// Reloads the account and folder list.
    pub fn reload_mailboxes(&mut self, ctx: Option<&egui::Context>) {
        let (Some(mail), Some(token)) = (self.mail.clone(), self.token()) else {
            return;
        };
        self.reader.mailboxes = Load::Loading;
        self.run(ctx, move || Event::Mailboxes(mail.mailboxes(&token)));
    }

    /// Selects a folder and loads its messages. Closes the open message.
    pub fn select_folder(&mut self, ctx: Option<&egui::Context>, folder_id: &str) {
        let (Some(mail), Some(token)) = (self.mail.clone(), self.token()) else {
            return;
        };
        self.reader.folder = Some(folder_id.to_string());
        self.reader.messages = Load::Loading;
        self.reader.open = None;
        self.reader.message = Load::Idle;
        let id = folder_id.to_string();
        self.run(ctx, move || {
            let result = mail.messages(&token, &id);
            Event::Messages(id, result)
        });
    }

    /// Opens a message in the reading pane.
    pub fn open_message(&mut self, ctx: Option<&egui::Context>, message_id: &str) {
        let (Some(mail), Some(token)) = (self.mail.clone(), self.token()) else {
            return;
        };
        self.reader.open = Some(message_id.to_string());
        self.reader.message = Load::Loading;
        let id = message_id.to_string();
        self.run(ctx, move || {
            let result = mail.message(&token, &id).map(Box::new);
            Event::Opened(id, result)
        });
    }

    /// A read call came back 401: the session is gone.
    fn session_ended(&mut self) {
        self.store.clear();
        self.reader = Reader::default();
        self.screen = Screen::SignIn;
        self.error = Some("Your session has ended. Sign in again.".into());
    }

    /// Applies finished calls. Called at the top of every frame.
    pub fn poll(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Restored(token, Ok(user_id)) => {
                    self.enter_shell(SignedIn { token, user_id });
                }
                Event::Restored(_, Err(AuthError::Unauthorized(_))) => {
                    self.store.clear();
                    self.screen = Screen::SignIn;
                    self.error = Some("Your session has ended. Sign in again.".into());
                }
                Event::Restored(_, Err(AuthError::Failed(message))) => {
                    // Keep the token: the server may only be down.
                    self.screen = Screen::SignIn;
                    self.error = Some(format!("Could not restore the session: {message}"));
                }
                Event::SignedIn(result) => {
                    self.busy = false;
                    match result {
                        Ok(session) => {
                            self.notice = self.store.save(&session.token).err();
                            self.form.password.clear();
                            self.error = None;
                            self.enter_shell(session);
                        }
                        Err(AuthError::Unauthorized(_)) => {
                            self.error = Some("Wrong username or password.".into());
                        }
                        Err(AuthError::Failed(message)) => {
                            self.error = Some(sentence(&message));
                        }
                    }
                }
                // Read results arriving after sign-out, or for a folder or
                // message no longer selected, are dropped.
                _ if self.token().is_none() => {}
                Event::Mailboxes(Err(AuthError::Unauthorized(_)))
                | Event::Messages(_, Err(AuthError::Unauthorized(_)))
                | Event::Opened(_, Err(AuthError::Unauthorized(_))) => self.session_ended(),
                Event::Mailboxes(result) => {
                    self.reader.mailboxes = loaded(result, "folders");
                }
                Event::Messages(id, result) => {
                    if self.reader.folder.as_deref() == Some(id.as_str()) {
                        self.reader.messages = loaded(result, "messages");
                    }
                }
                Event::Opened(id, result) => {
                    if self.reader.open.as_deref() == Some(id.as_str()) {
                        self.reader.message = loaded(result, "the message").map(|message| {
                            let doc = HtmlDocument::parse(&mail::body_html(&message));
                            Opened {
                                message: *message,
                                doc,
                            }
                        });
                    }
                }
            }
        }
    }

    /// Validates the form and starts `login` or `bootstrap`.
    pub fn submit(&mut self, ctx: Option<&egui::Context>) {
        if self.busy {
            return;
        }
        let credentials: Credentials = match self.form.submit() {
            Ok(credentials) => credentials,
            Err(message) => {
                self.error = Some(message);
                return;
            }
        };
        self.busy = true;
        self.error = None;
        let api = self.api.clone();
        self.run(ctx, move || Event::SignedIn(api.sign_in(&credentials)));
    }

    /// Drops the local session at once and tells the server in the
    /// background. A failed logout only leaves a server session to expire.
    pub fn sign_out(&mut self, ctx: Option<&egui::Context>) {
        let Screen::SignedIn(session) = std::mem::replace(&mut self.screen, Screen::SignIn) else {
            return;
        };
        self.store.clear();
        self.notice = None;
        self.reader = Reader::default();
        let api = self.api.clone();
        // Logout has no result the UI waits on, so it never goes through
        // the event channel.
        let job = move || {
            let _ = api.sign_out(&session.token);
        };
        match self.exec {
            Exec::Inline => job(),
            Exec::Thread => {
                let _ = ctx;
                std::thread::spawn(job);
            }
        }
    }

    /// Draws one frame.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.poll();
        match self.screen.clone() {
            Screen::Restoring => {
                ui.centered_and_justified(|ui| {
                    ui.add(fluent2_egui::Spinner::new().label("Restoring your session"));
                });
            }
            Screen::SignIn => self.sign_in_ui(ui),
            Screen::SignedIn(session) => self.shell_ui(ui, &session),
        }
    }

    fn sign_in_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let first_run = self.form.mode == AuthMode::FirstRun;
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.15);
            ui.set_max_width(320.0);
            ui.heading(if first_run {
                "Set up email-rs"
            } else {
                "Sign in to email-rs"
            });
            if first_run {
                ui.label("Create the first account. This works only on the server's own machine.");
            }
            ui.add_space(16.0);
            let mut enter = false;
            Field::new("Username").required(true).show(ui, |ui| {
                let response = ui.add(
                    Input::new(&mut self.form.username)
                        .label("Username")
                        .desired_width(320.0)
                        .id_salt("username"),
                );
                enter |= response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            });
            let validation = match &self.error {
                Some(message) => ValidationState::Error(message.clone()),
                None => ValidationState::None,
            };
            Field::new("Password")
                .required(true)
                .validation(validation)
                .show(ui, |ui| {
                    let response = ui.add(
                        Input::new(&mut self.form.password)
                            .label("Password")
                            .password()
                            .desired_width(320.0)
                            .id_salt("password"),
                    );
                    enter |= response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                });
            ui.add_space(12.0);
            let label = match (self.busy, first_run) {
                (true, _) => "Signing in…",
                (false, true) => "Create account",
                (false, false) => "Sign in",
            };
            let clicked = ui
                .add_enabled(
                    !self.busy,
                    Button::new(label)
                        .appearance(Appearance::Primary)
                        .min_width(320.0),
                )
                .clicked();
            if clicked || enter {
                self.submit(Some(&ctx));
            }
            ui.add_space(8.0);
            let toggle = if first_run {
                "I already have an account"
            } else {
                "First run? Set up the first account"
            };
            if ui
                .add_enabled(
                    !self.busy,
                    Button::new(toggle).appearance(Appearance::Transparent),
                )
                .clicked()
            {
                self.form.toggle_mode();
                self.error = None;
            }
        });
    }

    fn shell_ui(&mut self, ui: &mut egui::Ui, session: &SignedIn) {
        let ctx = ui.ctx().clone();
        egui::Panel::top("title_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("email-rs");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(Button::new("Sign out")).clicked() {
                        self.sign_out(Some(&ctx));
                    }
                    ui.label(format!("Signed in as {}", session.user_id));
                });
            });
        });
        if let Some(notice) = &self.notice {
            egui::Panel::bottom("status_bar").show(ui, |ui| {
                ui.label(notice.as_str());
            });
        }
        egui::Panel::left("folders")
            .resizable(true)
            .default_size(220.0)
            .show(ui, |ui| self.folders_ui(ui));
        egui::Panel::left("messages")
            .resizable(true)
            .default_size(320.0)
            .show(ui, |ui| self.messages_ui(ui));
        egui::CentralPanel::default().show(ui, |ui| self.reading_ui(ui));
    }

    fn folders_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            ui.strong("Folders");
            if self.mail.is_some()
                && ui
                    .add(Button::new("Refresh").appearance(Appearance::Subtle))
                    .clicked()
            {
                self.reload_mailboxes(Some(&ctx));
            }
        });
        let mut pick = None;
        match &self.reader.mailboxes {
            Load::Idle => {}
            Load::Loading => {
                ui.add(fluent2_egui::Spinner::new().label("Loading folders"));
            }
            Load::Failed(message) => {
                ui.label(message.as_str());
            }
            Load::Ready(mailboxes) if mailboxes.is_empty() => {
                ui.label("No accounts yet");
            }
            Load::Ready(mailboxes) => {
                egui::ScrollArea::vertical()
                    .id_salt("folder_list")
                    .show(ui, |ui| {
                        for mailbox in mailboxes {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(&mailbox.account.email).small());
                            for folder in mailbox.folders.iter().filter(|f| !f.is_excluded) {
                                let selected = self.reader.folder.as_deref() == Some(&folder.id);
                                let text = if folder.unread_count > 0 {
                                    format!("{} ({})", folder.name, folder.unread_count)
                                } else {
                                    folder.name.clone()
                                };
                                if ui.selectable_label(selected, text).clicked() && !selected {
                                    pick = Some(folder.id.clone());
                                }
                            }
                        }
                    });
            }
        }
        if let Some(id) = pick {
            self.select_folder(Some(&ctx), &id);
        }
    }

    fn messages_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut pick = None;
        match &self.reader.messages {
            Load::Idle => {
                ui.centered_and_justified(|ui| {
                    ui.label("No folder selected");
                });
            }
            Load::Loading => {
                ui.add(fluent2_egui::Spinner::new().label("Loading messages"));
            }
            Load::Failed(message) => {
                ui.label(message.as_str());
            }
            Load::Ready(messages) if messages.is_empty() => {
                ui.centered_and_justified(|ui| {
                    ui.label("This folder is empty");
                });
            }
            Load::Ready(messages) => {
                egui::ScrollArea::vertical()
                    .id_salt("message_list")
                    .show(ui, |ui| {
                        for message in messages {
                            let selected = self.reader.open.as_deref() == Some(&message.id);
                            let from = mail::sender(
                                message.from_name.as_deref(),
                                message.from_email.as_deref(),
                            );
                            let mut text = egui::RichText::new(format!(
                                "{from}\n{}",
                                mail::subject(message.subject.as_deref())
                            ));
                            if !message.is_read {
                                text = text.strong();
                            }
                            if ui.selectable_label(selected, text).clicked() && !selected {
                                pick = Some(message.id.clone());
                            }
                            ui.separator();
                        }
                    });
            }
        }
        if let Some(id) = pick {
            self.open_message(Some(&ctx), &id);
        }
    }

    fn reading_ui(&mut self, ui: &mut egui::Ui) {
        match &self.reader.message {
            Load::Idle => {
                ui.centered_and_justified(|ui| {
                    ui.label("No message selected");
                });
            }
            Load::Loading => {
                ui.add(fluent2_egui::Spinner::new().label("Opening the message"));
            }
            Load::Failed(message) => {
                ui.label(message.as_str());
            }
            Load::Ready(opened) => {
                let m = &opened.message;
                ui.heading(mail::subject(m.subject.as_deref()));
                let from = match (m.from_name.as_deref(), m.from_email.as_deref()) {
                    (Some(name), Some(email)) if !name.trim().is_empty() => {
                        format!("{name} <{email}>")
                    }
                    (name, email) => mail::sender(name, email),
                };
                ui.label(format!("From: {from}"));
                if let Some(to) = m.to.as_deref().filter(|t| !t.is_empty()) {
                    ui.label(format!("To: {to}"));
                }
                if let Some(date) = m.date.as_deref() {
                    ui.label(date);
                }
                ui.separator();
                let out = egui::ScrollArea::vertical()
                    .id_salt("reading_pane")
                    .show(ui, |ui| HtmlView::new("message_body", &opened.doc).show(ui))
                    .inner;
                if let Some(LinkTarget::Web(url)) = out.clicked.map(|c| c.target) {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
            }
        }
    }
}

impl eframe::App for EmailApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        EmailApp::ui(self, ui);
    }
}

/// A read result as a [`Load`], with a message naming what failed.
fn loaded<T>(result: Result<T, AuthError>, what: &str) -> Load<T> {
    match result {
        Ok(value) => Load::Ready(value),
        Err(err) => Load::Failed(format!("Could not load {what}: {}", err.message())),
    }
}

impl<T> Load<T> {
    fn map<U>(self, f: impl FnOnce(T) -> U) -> Load<U> {
        match self {
            Load::Idle => Load::Idle,
            Load::Loading => Load::Loading,
            Load::Ready(value) => Load::Ready(f(value)),
            Load::Failed(message) => Load::Failed(message),
        }
    }
}

/// Capitalises a server message for display.
fn sentence(message: &str) -> String {
    let mut chars = message.chars();
    match chars.next() {
        Some(first) => {
            let mut out: String = first.to_uppercase().collect();
            out.push_str(chars.as_str());
            if !out.ends_with('.') {
                out.push('.');
            }
            out
        }
        None => "Sign-in failed.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::sentence;

    #[test]
    fn sentence_capitalises_and_ends_with_a_stop() {
        assert_eq!(
            sentence("first-run setup is loopback-only"),
            "First-run setup is loopback-only."
        );
        assert_eq!(sentence("Done."), "Done.");
        assert_eq!(sentence(""), "Sign-in failed.");
    }
}
