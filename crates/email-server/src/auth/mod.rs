#![allow(dead_code)]

pub mod app;
pub mod oauth2;

use serde::{Deserialize, Serialize};

/// Returns the Google OAuth client ID.
/// Compile-time value (baked in at build) wins; falls back to runtime env var
/// so local dev with a .env file still works without a rebuild.
pub fn google_client_id() -> Option<String> {
    option_env!("GOOGLE_CLIENT_ID")
        .map(str::to_string)
        .or_else(|| std::env::var("GOOGLE_CLIENT_ID").ok())
}

pub fn google_client_secret() -> Option<String> {
    option_env!("GOOGLE_CLIENT_SECRET")
        .map(str::to_string)
        .or_else(|| std::env::var("GOOGLE_CLIENT_SECRET").ok())
}

pub fn microsoft_client_id() -> Option<String> {
    option_env!("MICROSOFT_CLIENT_ID")
        .map(str::to_string)
        .or_else(|| std::env::var("MICROSOFT_CLIENT_ID").ok())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthConfig {
    OAuth2 {
        client_id: String,
        client_secret: String,
        access_token: Option<String>,
        refresh_token: Option<String>,
        expires_at: Option<i64>,
    },
    Basic {
        username: String,
        password: String,
    },
    AppPassword {
        email: String,
        app_password: String,
    },
}
