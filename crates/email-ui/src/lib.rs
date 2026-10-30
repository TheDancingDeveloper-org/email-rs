//! fluent2-egui client for email-rs.
//!
//! The client signs in through the server's `app-auth` API (`login`,
//! first-run `bootstrap`, `me`, `logout`), keeps the bearer token in the OS
//! keyring and restores the session on the next launch. Signed in, it shows
//! each account's folders, the selected folder's messages and the selected
//! message in fluent2-egui's HtmlView.

pub mod app;
pub mod auth;
pub mod backend;
pub mod fixture;
pub mod mail;
pub mod session;

pub use app::{EmailApp, Exec, Screen};
pub use auth::{AuthMode, LoginForm};
pub use backend::{AuthApi, AuthError, HttpAuth, SignedIn};
pub use mail::{MailApi, Mailbox};
pub use session::{KeyringStore, MemoryStore, TokenStore};
