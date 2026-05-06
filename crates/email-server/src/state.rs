use std::collections::HashMap;
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::{broadcast, Mutex};

/// Shared application state threaded through all Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    /// True when the messages_fts FTS5 table was created successfully.
    pub has_fts: bool,
    /// Pending OAuth2 state tokens. Key = state string, value = PKCE code_verifier (None for non-PKCE flows).
    pub oauth_states: Arc<Mutex<HashMap<String, Option<String>>>>,
    /// Broadcast channel for push-notifying connected SSE clients of sync events.
    pub event_tx: broadcast::Sender<String>,
}

impl AppState {
    pub fn new(pool: SqlitePool, has_fts: bool) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            pool,
            has_fts,
            oauth_states: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }
}
