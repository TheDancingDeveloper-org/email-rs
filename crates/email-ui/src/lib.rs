//! fluent2-egui client for email-rs.
//!
//! F1 slice one: the app shell and sign-in. The client signs in through the
//! server's `app-auth` API (`login`, first-run `bootstrap`, `me`, `logout`),
//! keeps the bearer token in the OS keyring and restores the session on the
//! next launch. Folders, the message list and the reading pane come next.

pub mod app;
pub mod auth;
pub mod backend;
pub mod fixture;
pub mod session;

pub use app::{EmailApp, Exec, Screen};
pub use auth::{AuthMode, LoginForm};
pub use backend::{AuthApi, AuthError, HttpAuth, SignedIn};
pub use session::{KeyringStore, MemoryStore, TokenStore};
