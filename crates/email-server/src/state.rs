use std::collections::HashMap;
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::{broadcast, Mutex};

use crate::auth::app::AuthSettings;
use crate::config::WebhookPolicy;

#[derive(Clone, Debug)]
pub struct OAuthState {
    pub user_id: String,
    pub session_id: String,
    pub verifier: Option<String>,
    pub account_id: Option<String>,
}

/// Shared application state threaded through all Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    /// True when the messages_fts FTS5 table was created successfully.
    pub has_fts: bool,
    /// Pending OAuth2 state tokens, including the authenticated application user
    /// that initiated the flow and its optional re-authorization account.
    pub oauth_states: Arc<Mutex<HashMap<String, OAuthState>>>,
    /// Broadcast channel for push-notifying connected SSE clients of sync events.
    pub event_tx: broadcast::Sender<String>,
    pub auth: AuthSettings,
    /// SSRF policy applied when a webhook URL is stored.
    pub webhook_policy: WebhookPolicy,
}

impl AppState {
    pub fn new(pool: SqlitePool, has_fts: bool) -> Self {
        Self::with_auth(pool, has_fts, AuthSettings::disabled())
    }

    pub fn with_auth(pool: SqlitePool, has_fts: bool, auth: AuthSettings) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            pool,
            has_fts,
            oauth_states: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
            auth,
            webhook_policy: WebhookPolicy::from_env(),
        }
    }
}
