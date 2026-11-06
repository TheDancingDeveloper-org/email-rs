use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr},
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use argon2::{
    password_hash::{phc::PasswordHash, PasswordHasher, PasswordVerifier},
    Argon2,
};
use axum::{
    body::Body,
    extract::{ConnectInfo, Path, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

use tokio::sync::Semaphore;

use crate::{error::AppError, state::AppState};

pub const SESSION_COOKIE: &str = "email_rs_session";
const LOGIN_FAILURE_LIMIT: u32 = 5;
const LOGIN_THROTTLE_CAPACITY: usize = 4096;
const ARGON2_CONCURRENCY: usize = 4;
const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$BwcHBwcHBwcHBwcHBwcHBw$WLRGUa13/B3MXznFse33FId4O++DaYL6ulYUMEhpRqE";

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum LoginKey {
    Account(String),
    Ip(IpAddr),
}

#[derive(Clone, Debug)]
struct FailedLogins {
    failures: u32,
    blocked_until: Option<Instant>,
    touched_at: Instant,
}

/// Bounded failed-login state. Entries are deliberately process-local: it
/// protects this single-user server without adding attacker-controlled rows to
/// its database.
#[derive(Debug)]
pub struct LoginThrottle {
    entries: HashMap<LoginKey, FailedLogins>,
    capacity: usize,
}

impl Default for LoginThrottle {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            capacity: LOGIN_THROTTLE_CAPACITY,
        }
    }
}

impl LoginThrottle {
    fn is_blocked(&mut self, account: &str, ip: Option<IpAddr>, now: Instant) -> bool {
        [
            Some(LoginKey::Account(account.to_owned())),
            ip.map(LoginKey::Ip),
        ]
        .into_iter()
        .flatten()
        .any(|key| {
            self.entries.get_mut(&key).is_some_and(|entry| {
                entry.touched_at = now;
                entry.blocked_until.is_some_and(|until| until > now)
            })
        })
    }

    fn record_failure(&mut self, account: &str, ip: Option<IpAddr>, now: Instant) {
        for key in [
            Some(LoginKey::Account(account.to_owned())),
            ip.map(LoginKey::Ip),
        ]
        .into_iter()
        .flatten()
        {
            self.make_room_for(&key);
            let entry = self.entries.entry(key).or_insert(FailedLogins {
                failures: 0,
                blocked_until: None,
                touched_at: now,
            });
            entry.failures = entry.failures.saturating_add(1);
            entry.touched_at = now;
            if entry.failures >= LOGIN_FAILURE_LIMIT {
                let exponent = (entry.failures - LOGIN_FAILURE_LIMIT).min(6);
                entry.blocked_until = Some(now + Duration::from_secs(1 << exponent));
            }
        }
    }

    fn record_success(&mut self, account: &str, ip: Option<IpAddr>) {
        self.entries.remove(&LoginKey::Account(account.to_owned()));
        if let Some(ip) = ip {
            self.entries.remove(&LoginKey::Ip(ip));
        }
    }

    fn make_room_for(&mut self, key: &LoginKey) {
        if self.entries.len() < self.capacity || self.entries.contains_key(key) {
            return;
        }
        if let Some(oldest) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.touched_at)
            .map(|(key, _)| key.clone())
        {
            self.entries.remove(&oldest);
        }
    }
}

#[derive(Clone, Debug)]
pub struct AuthSettings {
    pub enabled: bool,
    pub secure_cookies: bool,
    pub public_url: Option<String>,
    pub allowed_origins: Arc<HashSet<String>>,
    pub session_ttl_secs: i64,
    pub bearer_ttl_secs: i64,
}

impl AuthSettings {
    pub fn from_env(public_url: Option<&str>) -> Self {
        let mut allowed_origins: HashSet<String> = std::env::var("EMAIL_RS_ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
            .map(str::to_string)
            .collect();
        if let Some(url) = public_url {
            allowed_origins.insert(url.trim_end_matches('/').to_string());
        }
        Self {
            enabled: !env_flag("EMAIL_RS_DISABLE_AUTH"),
            secure_cookies: public_url.is_some_and(|url| url.starts_with("https://"))
                || env_flag("EMAIL_RS_COOKIE_SECURE"),
            public_url: public_url.map(|url| url.trim_end_matches('/').to_string()),
            allowed_origins: Arc::new(allowed_origins),
            session_ttl_secs: env_i64("EMAIL_RS_SESSION_TTL_SECS", 12 * 60 * 60),
            bearer_ttl_secs: env_i64("EMAIL_RS_BEARER_TTL_SECS", 30 * 24 * 60 * 60),
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            secure_cookies: false,
            public_url: None,
            allowed_origins: Arc::new(HashSet::new()),
            session_ttl_secs: 12 * 60 * 60,
            bearer_ttl_secs: 30 * 24 * 60 * 60,
        }
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

fn env_i64(name: &str, default: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[derive(Clone, Debug)]
pub struct AuthPrincipal {
    pub user_id: String,
    pub session_id: String,
    pub kind: PrincipalKind,
    pub scopes: Arc<HashSet<String>>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    Cookie,
    Bearer,
    Integration,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Cookie,
    Bearer,
}

impl ClientKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cookie => "cookie",
            Self::Bearer => "bearer",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    pub username: String,
    pub password: String,
    #[serde(default = "default_client_kind")]
    pub client: ClientKind,
}

fn default_client_kind() -> ClientKind {
    ClientKind::Cookie
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthResponse {
    user_id: String,
    session_id: String,
    expires_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct UserPassword {
    id: String,
    password_hash: String,
}

#[derive(Debug, sqlx::FromRow)]
struct SessionRow {
    id: String,
    user_id: String,
    kind: String,
}

pub async fn bootstrap_from_env(pool: &SqlitePool) -> anyhow::Result<()> {
    let Some(password) = std::env::var("EMAIL_RS_BOOTSTRAP_PASSWORD")
        .ok()
        .filter(|password| !password.is_empty())
    else {
        return Ok(());
    };
    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_users")
        .fetch_one(pool)
        .await?;
    if existing != 0 {
        return Ok(());
    }
    validate_password(&password).map_err(anyhow::Error::msg)?;
    let username =
        std::env::var("EMAIL_RS_BOOTSTRAP_USERNAME").unwrap_or_else(|_| "admin".to_string());
    create_user(pool, &username, &password).await?;
    tracing::info!("created initial application user from EMAIL_RS_BOOTSTRAP_PASSWORD");
    Ok(())
}

pub async fn bootstrap(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(credentials): Json<Credentials>,
) -> Response {
    if !peer.ip().is_loopback() || has_forwarding_headers(&headers) {
        return error(StatusCode::FORBIDDEN, "first-run setup is loopback-only");
    }
    if let Err(message) = validate_password(&credentials.password) {
        return error(StatusCode::BAD_REQUEST, message);
    }
    // Hash before taking the write lock so the lock isn't held while hashing.
    let hash = match hash_password(credentials.password).await {
        Ok(hash) => hash,
        Err(err) => return err.into_response(),
    };
    // Read-then-insert: take the write lock up front (see `db::begin_write`).
    let mut tx = match crate::db::begin_write(&state.pool).await {
        Ok(tx) => tx,
        Err(err) => return AppError::Database(err).into_response(),
    };
    let count: Result<i64, _> = sqlx::query_scalar("SELECT COUNT(*) FROM app_users")
        .fetch_one(&mut *tx)
        .await;
    match count {
        Ok(0) => {}
        Ok(_) => return error(StatusCode::CONFLICT, "application user already exists"),
        Err(err) => return AppError::Database(err).into_response(),
    }
    let user_id = Uuid::new_v4().to_string();
    if let Err(err) =
        sqlx::query("INSERT INTO app_users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user_id)
            .bind(credentials.username.trim())
            .bind(hash)
            .execute(&mut *tx)
            .await
    {
        return AppError::Database(err).into_response();
    }
    if let Err(err) = tx.commit().await {
        return AppError::Database(err).into_response();
    }
    issue_session(&state, user_id, credentials.client).await
}

fn has_forwarding_headers(headers: &HeaderMap) -> bool {
    [
        "forwarded",
        "x-forwarded-for",
        "x-forwarded-host",
        "x-forwarded-proto",
        "x-real-ip",
        "cf-connecting-ip",
        "true-client-ip",
        "x-client-ip",
    ]
    .iter()
    .any(|name| headers.contains_key(*name))
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Json(credentials): Json<Credentials>,
) -> Response {
    let account = credentials.username.trim().to_owned();
    let peer_ip = peer.map(|Extension(ConnectInfo(peer))| peer.ip());
    if state
        .login_throttle
        .lock()
        .await
        .is_blocked(&account, peer_ip, Instant::now())
    {
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "login temporarily unavailable",
        );
    }
    let user = match sqlx::query_as::<_, UserPassword>(
        "SELECT id, password_hash FROM app_users WHERE username = ?",
    )
    .bind(&account)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(user) => user,
        Err(err) => return AppError::Database(err).into_response(),
    };
    // Always run Argon2, including for an unknown account, so response timing
    // does not disclose whether a username exists.
    let encoded = user.as_ref().map_or_else(
        || DUMMY_PASSWORD_HASH.to_owned(),
        |user| user.password_hash.clone(),
    );
    match verify_password(credentials.password, encoded).await {
        Ok(true) if user.is_some() => {
            state
                .login_throttle
                .lock()
                .await
                .record_success(&account, peer_ip);
            issue_session(&state, user.expect("checked above").id, credentials.client).await
        }
        Ok(_) => {
            state
                .login_throttle
                .lock()
                .await
                .record_failure(&account, peer_ip, Instant::now());
            error(StatusCode::UNAUTHORIZED, "invalid username or password")
        }
        Err(err) => err.into_response(),
    }
}

pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<AuthPrincipal>,
) -> Response {
    if let Err(err) = sqlx::query("DELETE FROM app_sessions WHERE id = ?")
        .bind(&principal.session_id)
        .execute(&state.pool)
        .await
    {
        return AppError::Database(err).into_response();
    }
    let kind = match principal.kind {
        PrincipalKind::Cookie => ClientKind::Cookie,
        PrincipalKind::Bearer => ClientKind::Bearer,
        PrincipalKind::Integration => {
            return error(
                StatusCode::FORBIDDEN,
                "integration tokens cannot be refreshed",
            )
        }
    };
    issue_session(&state, principal.user_id, kind).await
}

pub async fn logout(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<AuthPrincipal>,
) -> Response {
    if let Err(err) = sqlx::query("DELETE FROM app_sessions WHERE id = ? AND user_id = ?")
        .bind(&principal.session_id)
        .bind(&principal.user_id)
        .execute(&state.pool)
        .await
    {
        return AppError::Database(err).into_response();
    }
    let mut response = Json(serde_json::json!({ "status": "logged_out" })).into_response();
    expire_cookie(response.headers_mut(), state.auth.secure_cookies);
    response
}

pub async fn revoke(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<AuthPrincipal>,
    Path(session_id): Path<String>,
) -> Response {
    match sqlx::query("DELETE FROM app_sessions WHERE id = ? AND user_id = ?")
        .bind(&session_id)
        .bind(&principal.user_id)
        .execute(&state.pool)
        .await
    {
        Ok(result) if result.rows_affected() == 0 => {
            error(StatusCode::NOT_FOUND, "session not found")
        }
        Ok(_) => Json(serde_json::json!({ "status": "revoked" })).into_response(),
        Err(err) => AppError::Database(err).into_response(),
    }
}

pub async fn me(Extension(principal): Extension<AuthPrincipal>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "userId": principal.user_id,
        "sessionId": principal.session_id,
        "client": principal.kind,
    }))
}

pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    if !state.auth.enabled {
        return next.run(request).await;
    }
    let (raw_token, presented_kind) = match presented_token(request.headers()) {
        Some(token) => token,
        None => return error(StatusCode::UNAUTHORIZED, "authentication required"),
    };
    let token_hash = token_hash(&raw_token);
    let now = chrono::Utc::now().timestamp();
    let session = match sqlx::query_as::<_, SessionRow>(
        "SELECT id, user_id, kind FROM app_sessions WHERE token_hash = ? AND expires_at > ?",
    )
    .bind(&token_hash)
    .bind(now)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(session) => session,
        Err(err) => return AppError::Database(err).into_response(),
    };
    let principal = if let Some(session) = session {
        let stored_kind = if session.kind == "bearer" {
            ClientKind::Bearer
        } else {
            ClientKind::Cookie
        };
        if stored_kind != presented_kind {
            return error(StatusCode::UNAUTHORIZED, "invalid authentication mechanism");
        }
        if stored_kind == ClientKind::Cookie
            && is_mutation(request.method())
            && !valid_origin(request.headers(), &state.auth)
        {
            return error(StatusCode::FORBIDDEN, "untrusted request origin");
        }
        let _ = sqlx::query("UPDATE app_sessions SET last_seen_at = datetime('now') WHERE id = ?")
            .bind(&session.id)
            .execute(&state.pool)
            .await;
        AuthPrincipal {
            user_id: session.user_id,
            session_id: session.id,
            kind: if stored_kind == ClientKind::Bearer {
                PrincipalKind::Bearer
            } else {
                PrincipalKind::Cookie
            },
            scopes: Arc::new(HashSet::new()),
        }
    } else {
        if presented_kind != ClientKind::Bearer {
            return error(StatusCode::UNAUTHORIZED, "session expired or revoked");
        }
        #[derive(sqlx::FromRow)]
        struct IntegrationRow {
            id: String,
            user_id: String,
            scopes_json: String,
        }
        let integration = match sqlx::query_as::<_, IntegrationRow>(
            r#"SELECT id, user_id, scopes_json FROM integration_tokens
               WHERE token_hash = ? AND revoked_at IS NULL
                 AND (expires_at IS NULL OR expires_at > ?)"#,
        )
        .bind(&token_hash)
        .bind(now)
        .fetch_optional(&state.pool)
        .await
        {
            Ok(Some(integration)) => integration,
            Ok(None) => return error(StatusCode::UNAUTHORIZED, "credential expired or revoked"),
            Err(err) => return AppError::Database(err).into_response(),
        };
        let scopes: HashSet<String> = match serde_json::from_str(&integration.scopes_json) {
            Ok(scopes) => scopes,
            Err(_) => {
                return error(
                    StatusCode::UNAUTHORIZED,
                    "integration credential is invalid",
                )
            }
        };
        let Some(required_scope) =
            required_integration_scope(request.method(), request.uri().path())
        else {
            return error(
                StatusCode::FORBIDDEN,
                "integration credentials cannot access this route",
            );
        };
        if !scopes.contains(required_scope) {
            return error(
                StatusCode::FORBIDDEN,
                format!("integration token requires scope {required_scope}"),
            );
        }
        let _ = sqlx::query(
            "UPDATE integration_tokens SET last_used_at = datetime('now') WHERE id = ?",
        )
        .bind(&integration.id)
        .execute(&state.pool)
        .await;
        AuthPrincipal {
            user_id: integration.user_id,
            session_id: integration.id,
            kind: PrincipalKind::Integration,
            scopes: Arc::new(scopes),
        }
    };
    request.extensions_mut().insert(principal);
    next.run(request).await
}

fn required_integration_scope(method: &Method, path: &str) -> Option<&'static str> {
    if *method == Method::POST && path == "/messages" {
        return Some("mail.send");
    }
    if matches!(*method, Method::POST | Method::PUT | Method::DELETE)
        && (path == "/drafts" || path.starts_with("/drafts/"))
    {
        return Some("mail.modify");
    }
    if *method == Method::POST && path.starts_with("/sync/") {
        return Some("mail.sync");
    }
    if *method == Method::GET
        && (path == "/auth/gmail/authorize" || path == "/auth/microsoft/authorize")
    {
        // Start a provider sign-in for a new or existing (`account_id`) mailbox.
        // The pending OAuth state is bound to this token, so revoking the token
        // also invalidates any sign-in it started.
        return Some("mail.accounts.connect");
    }
    if *method == Method::POST && path == "/accounts/discover" {
        return Some("mail.accounts.connect");
    }
    if path.starts_with("/accounts") {
        return (*method == Method::GET).then_some("mail.accounts.read");
    }
    if *method == Method::GET
        && path.starts_with("/messages/")
        && (path.matches('/').count() == 2 || path.contains("/attachments"))
    {
        return Some("mail.body.read");
    }
    if *method == Method::GET
        && (path.starts_with("/folders/")
            || path == "/integration-events"
            || path == "/search"
            || path == "/search/suggest"
            || path == "/contacts/suggest"
            || path.starts_with("/smart-folders/")
            || path == "/labels"
            || path.starts_with("/labels/"))
    {
        return Some("mail.metadata.read");
    }
    if matches!(
        *method,
        Method::POST | Method::PATCH | Method::PUT | Method::DELETE
    ) && (path.starts_with("/messages/")
        || path == "/messages/bulk"
        || path.starts_with("/folders/")
        || path.starts_with("/labels/"))
    {
        return Some("mail.modify");
    }
    None
}

fn presented_token(headers: &HeaderMap) -> Option<(String, ClientKind)> {
    if let Some(token) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|token| !token.is_empty())
    {
        return Some((token.to_string(), ClientKind::Bearer));
    }
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let (name, value) = cookie.trim().split_once('=')?;
                (name == SESSION_COOKIE && !value.is_empty()).then(|| value.to_string())
            })
        })
        .map(|token| (token, ClientKind::Cookie))
}

fn is_mutation(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

fn valid_origin(headers: &HeaderMap, settings: &AuthSettings) -> bool {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    if settings.allowed_origins.contains(origin) {
        return true;
    }
    if let Some(public_url) = settings.public_url.as_deref() {
        return origin == public_url;
    }
    tracing::warn!("EMAIL_RS_PUBLIC_URL is unset; deriving request origin from headers");
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let hostname = host.rsplit_once(':').map_or(host, |(hostname, _)| hostname);
    if !matches!(hostname, "127.0.0.1" | "localhost" | "[::1]" | "::1") {
        return false;
    }
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .filter(|scheme| matches!(*scheme, "http" | "https"))
        .unwrap_or(if settings.secure_cookies {
            "https"
        } else {
            "http"
        });
    origin == format!("{scheme}://{host}")
}

async fn issue_session(state: &AppState, user_id: String, kind: ClientKind) -> Response {
    let raw_token = new_token();
    let session_id = Uuid::new_v4().to_string();
    let ttl = match kind {
        ClientKind::Cookie => state.auth.session_ttl_secs,
        ClientKind::Bearer => state.auth.bearer_ttl_secs,
    };
    let expires_at = chrono::Utc::now().timestamp() + ttl;
    if let Err(err) = sqlx::query(
        "INSERT INTO app_sessions (id, user_id, token_hash, kind, expires_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&session_id)
    .bind(&user_id)
    .bind(token_hash(&raw_token))
    .bind(kind.as_str())
    .bind(expires_at)
    .execute(&state.pool)
    .await
    {
        return AppError::Database(err).into_response();
    }
    let mut response = Json(AuthResponse {
        user_id,
        session_id,
        expires_at,
        token: (kind == ClientKind::Bearer).then_some(raw_token.clone()),
    })
    .into_response();
    if kind == ClientKind::Cookie {
        set_cookie(
            response.headers_mut(),
            &raw_token,
            ttl,
            state.auth.secure_cookies,
        );
    }
    response
}

async fn create_user(pool: &SqlitePool, username: &str, password: &str) -> anyhow::Result<String> {
    let hash = hash_password(password.to_string())
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO app_users (id, username, password_hash) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(username.trim())
        .bind(hash)
        .execute(pool)
        .await?;
    Ok(id)
}

fn validate_password(password: &str) -> Result<(), &'static str> {
    if password.len() < 12 {
        Err("password must contain at least 12 characters")
    } else {
        Ok(())
    }
}

async fn hash_password(password: String) -> Result<String, AppError> {
    let permit = argon2_semaphore()
        .acquire()
        .await
        .map_err(|_| AppError::Auth("password hashing unavailable".into()))?;
    tokio::task::spawn_blocking(move || {
        // A random v4 UUID is a 16-byte salt, the PHC-recommended length.
        Argon2::default()
            .hash_password_with_salt(password.as_bytes(), Uuid::new_v4().as_bytes())
            .map(|hash| hash.to_string())
            .map_err(|error| AppError::Auth(format!("password hashing failed: {error}")))
    })
    .await
    .map_err(|error| AppError::Auth(format!("password hashing task failed: {error}")))?
    .inspect(|_| drop(permit))
}

async fn verify_password(password: String, encoded: String) -> Result<bool, AppError> {
    let permit = argon2_semaphore()
        .acquire()
        .await
        .map_err(|_| AppError::Auth("password verification unavailable".into()))?;
    tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&encoded)
            .map_err(|error| AppError::Auth(format!("stored password hash is invalid: {error}")))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    })
    .await
    .map_err(|error| AppError::Auth(format!("password verification task failed: {error}")))?
    .inspect(|_| drop(permit))
}

fn argon2_semaphore() -> &'static Semaphore {
    static SEMAPHORE: OnceLock<Semaphore> = OnceLock::new();
    SEMAPHORE.get_or_init(|| Semaphore::new(ARGON2_CONCURRENCY))
}

fn new_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub(crate) fn token_hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn set_cookie(headers: &mut HeaderMap, token: &str, max_age: i64, secure: bool) {
    let secure = if secure { "; Secure" } else { "" };
    let value = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure}"
    );
    if let Ok(value) = HeaderValue::from_str(&value) {
        headers.append(header::SET_COOKIE, value);
    }
}

fn expire_cookie(headers: &mut HeaderMap, secure: bool) {
    let secure = if secure { "; Secure" } else { "" };
    let value = format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{secure}");
    if let Ok(value) = HeaderValue::from_str(&value) {
        headers.append(header::SET_COOKIE, value);
    }
}

fn error(status: StatusCode, message: impl Into<String>) -> Response<Body> {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use axum::{
        body::Body,
        http::{header, Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    async fn setup() -> Arc<AppState> {
        let _db_dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let path = _db_dir.path().join("email.db").display().to_string();
        let (pool, has_fts) = crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap();
        let settings = AuthSettings {
            enabled: true,
            secure_cookies: false,
            public_url: None,
            allowed_origins: Arc::new(HashSet::from([
                "http://localhost:8585".to_string(),
                "capacitor://localhost".to_string(),
            ])),
            session_ttl_secs: 3600,
            bearer_ttl_secs: 3600,
        };
        Arc::new(AppState::with_auth(pool, has_fts, settings))
    }

    async fn seed_user(state: &AppState) {
        create_user(&state.pool, "admin", "correct horse battery")
            .await
            .unwrap();
    }

    async fn send(
        router: axum::Router,
        method: Method,
        uri: &str,
        body: Option<serde_json::Value>,
        headers: &[(&str, &str)],
    ) -> (StatusCode, HeaderMap, serde_json::Value) {
        let mut builder = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        if body.is_some() {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
        }
        let response = router
            .oneshot(
                builder
                    .body(body.map_or_else(Body::empty, |body| {
                        Body::from(serde_json::to_vec(&body).unwrap())
                    }))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, headers, body)
    }

    async fn login_as(state: &Arc<AppState>, kind: ClientKind) -> (HeaderMap, serde_json::Value) {
        let (status, headers, body) = send(
            crate::api::router(Arc::clone(state)),
            Method::POST,
            "/app-auth/login",
            Some(serde_json::json!({
                "username": "admin",
                "password": "correct horse battery",
                "client": kind,
            })),
            &[],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        (headers, body)
    }

    #[tokio::test]
    async fn bootstrap_rejects_loopback_peer_behind_reverse_proxy() {
        let state = setup().await;
        let credentials = || Credentials {
            username: "owner".into(),
            password: "correct horse battery".into(),
            client: ClientKind::Bearer,
        };

        for (name, value) in [
            ("forwarded", "for=203.0.113.10;proto=https"),
            ("x-forwarded-for", "203.0.113.10"),
            ("x-forwarded-host", "mail.example.com"),
            ("x-forwarded-proto", "https"),
            ("x-real-ip", "203.0.113.10"),
            ("cf-connecting-ip", "203.0.113.10"),
            ("true-client-ip", "203.0.113.10"),
            ("x-client-ip", "203.0.113.10"),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(name, value.parse().unwrap());
            let response = bootstrap(
                State(Arc::clone(&state)),
                ConnectInfo("127.0.0.1:43210".parse().unwrap()),
                headers,
                Json(credentials()),
            )
            .await;
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{name}");
        }

        let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_users")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(users, 0);
    }

    #[tokio::test]
    async fn bootstrap_still_allows_a_direct_loopback_peer() {
        let state = setup().await;
        let response = bootstrap(
            State(Arc::clone(&state)),
            ConnectInfo("[::1]:43210".parse().unwrap()),
            HeaderMap::new(),
            Json(Credentials {
                username: "owner".into(),
                password: "correct horse battery".into(),
                client: ClientKind::Bearer,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn login_is_throttled_after_repeated_account_failures() {
        let state = setup().await;
        seed_user(&state).await;
        for attempt in 0..LOGIN_FAILURE_LIMIT {
            let (status, _, body) = send(
                crate::api::router(Arc::clone(&state)),
                Method::POST,
                "/app-auth/login",
                Some(serde_json::json!({
                    "username": "admin",
                    "password": "definitely not the password",
                })),
                &[],
            )
            .await;
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "attempt {attempt}: {body}"
            );
            assert_eq!(body["error"], "invalid username or password");
        }
        let (status, _, body) = send(
            crate::api::router(state),
            Method::POST,
            "/app-auth/login",
            Some(serde_json::json!({
                "username": "admin",
                "password": "correct horse battery",
            })),
            &[],
        )
        .await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
        assert_eq!(body["error"], "login temporarily unavailable");
    }

    #[tokio::test]
    async fn unknown_and_known_accounts_return_the_same_login_error() {
        let state = setup().await;
        seed_user(&state).await;
        let mut errors = Vec::new();
        for username in ["admin", "missing-user"] {
            let (status, _, body) = send(
                crate::api::router(Arc::clone(&state)),
                Method::POST,
                "/app-auth/login",
                Some(serde_json::json!({
                    "username": username,
                    "password": "definitely not the password",
                })),
                &[],
            )
            .await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
            errors.push(body);
        }
        assert_eq!(errors[0], errors[1]);
    }

    #[test]
    fn login_throttle_is_bounded_and_tracks_ip_independently() {
        let mut throttle = LoginThrottle {
            entries: HashMap::new(),
            capacity: 8,
        };
        let now = Instant::now();
        let ip: IpAddr = "192.0.2.10".parse().unwrap();
        for index in 0..LOGIN_FAILURE_LIMIT {
            throttle.record_failure(&format!("user-{index}"), Some(ip), now);
        }
        assert!(throttle.entries.len() <= 8);
        assert!(throttle.is_blocked("another-user", Some(ip), now));
        for index in 5..20 {
            throttle.record_failure(&format!("user-{index}"), None, now);
        }
        assert!(throttle.entries.len() <= 8);
    }

    #[test]
    fn argon2_work_is_capped() {
        assert_eq!(argon2_semaphore().available_permits(), ARGON2_CONCURRENCY);
    }

    #[tokio::test]
    async fn password_hashes_round_trip_and_legacy_hashes_still_verify() {
        let hash = hash_password("correct horse battery".into()).await.unwrap();
        assert!(hash.starts_with("$argon2id$v=19$"));
        assert!(
            verify_password("correct horse battery".into(), hash.clone())
                .await
                .unwrap()
        );
        assert!(!verify_password("wrong".into(), hash).await.unwrap());

        // Produced by argon2 0.5 (the version stored users were hashed with)
        // from a fixed salt; argon2 0.6 must keep verifying it.
        let legacy = "$argon2id$v=19$m=19456,t=2,p=1$BwcHBwcHBwcHBwcHBwcHBw$WLRGUa13/B3MXznFse33FId4O++DaYL6ulYUMEhpRqE";
        assert!(
            verify_password("correct horse battery".into(), legacy.into())
                .await
                .unwrap()
        );
        assert!(!verify_password("wrong".into(), legacy.into())
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn anonymous_privileged_route_is_rejected_but_health_is_public() {
        let state = setup().await;
        let (status, _, _) = send(
            crate::api::router(Arc::clone(&state)),
            Method::GET,
            "/accounts",
            None,
            &[],
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let (status, _, _) =
            send(crate::api::router(state), Method::GET, "/health", None, &[]).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn every_inventory_route_is_protected() {
        let state = setup().await;
        for (method, template) in crate::api::PRIVILEGED_ROUTE_INVENTORY {
            let mut uri = template.to_string();
            while let Some(start) = uri.find('{') {
                let end = uri[start..].find('}').map(|offset| start + offset).unwrap();
                uri.replace_range(start..=end, "example");
            }
            let method = Method::from_bytes(method.as_bytes()).unwrap();
            let (status, _, body) = send(
                crate::api::router(Arc::clone(&state)),
                method.clone(),
                &uri,
                None,
                &[],
            )
            .await;
            assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "{method} {template}: {body}"
            );
        }
    }

    #[tokio::test]
    async fn cookie_login_uses_security_attributes_and_unlocks_reads() {
        let state = setup().await;
        seed_user(&state).await;
        let (headers, body) = login_as(&state, ClientKind::Cookie).await;
        assert!(body.get("token").is_none());
        let set_cookie = headers[header::SET_COOKIE].to_str().unwrap();
        assert!(set_cookie.contains("HttpOnly"));
        assert!(set_cookie.contains("SameSite=Strict"));
        let cookie = set_cookie.split(';').next().unwrap();

        let (status, _, body) = send(
            crate::api::router(state),
            Method::GET,
            "/accounts",
            None,
            &[("cookie", cookie)],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    #[tokio::test]
    async fn expired_and_revoked_bearer_tokens_are_rejected() {
        let state = setup().await;
        seed_user(&state).await;
        let (_, body) = login_as(&state, ClientKind::Bearer).await;
        let token = body["token"].as_str().unwrap();
        let session_id = body["sessionId"].as_str().unwrap();

        sqlx::query("UPDATE app_sessions SET expires_at = 0 WHERE id = ?")
            .bind(session_id)
            .execute(&state.pool)
            .await
            .unwrap();
        let authorization = format!("Bearer {token}");
        let (status, _, _) = send(
            crate::api::router(Arc::clone(&state)),
            Method::GET,
            "/accounts",
            None,
            &[("authorization", &authorization)],
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let (_, body) = login_as(&state, ClientKind::Bearer).await;
        let token = body["token"].as_str().unwrap();
        let session_id = body["sessionId"].as_str().unwrap();
        sqlx::query("DELETE FROM app_sessions WHERE id = ?")
            .bind(session_id)
            .execute(&state.pool)
            .await
            .unwrap();
        let authorization = format!("Bearer {token}");
        let (status, _, _) = send(
            crate::api::router(state),
            Method::GET,
            "/accounts",
            None,
            &[("authorization", &authorization)],
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn cookie_mutations_require_a_trusted_origin() {
        let state = setup().await;
        seed_user(&state).await;
        let (headers, _) = login_as(&state, ClientKind::Cookie).await;
        let cookie = headers[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap();

        let (status, _, _) = send(
            crate::api::router(Arc::clone(&state)),
            Method::POST,
            "/app-auth/logout",
            None,
            &[("cookie", cookie), ("origin", "https://evil.example")],
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, _, _) = send(
            crate::api::router(state),
            Method::POST,
            "/app-auth/logout",
            None,
            &[
                ("cookie", cookie),
                ("origin", "http://localhost:8585"),
                ("host", "localhost:8585"),
            ],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn hostile_origins_receive_no_cors_read_permission() {
        let state = setup().await;
        let app = crate::build_app(state, "/tmp/email-rs-no-static-files");
        let (status, headers, _) = send(
            app,
            Method::GET,
            "/api/v1/health",
            None,
            &[("origin", "https://evil.example")],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    }

    #[tokio::test]
    async fn responses_carry_security_headers() {
        let state = setup().await;
        let app = crate::build_app(state, "/tmp/email-rs-no-static-files");
        let (status, headers, _) = send(app, Method::GET, "/api/v1/health", None, &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers["x-content-type-options"], "nosniff");
        assert_eq!(headers["x-frame-options"], "DENY");
        assert_eq!(headers["referrer-policy"], "no-referrer");
        let csp = headers["content-security-policy"].to_str().unwrap();
        assert!(csp.contains("frame-ancestors 'none'"));
    }

    #[tokio::test]
    async fn oauth_state_owner_must_still_have_an_active_session() {
        let state = setup().await;
        seed_user(&state).await;
        let (_, body) = login_as(&state, ClientKind::Bearer).await;
        let pending = crate::state::OAuthState {
            user_id: body["userId"].as_str().unwrap().to_string(),
            session_id: body["sessionId"].as_str().unwrap().to_string(),
            verifier: Some("verifier".to_string()),
            account_id: Some("account-1".to_string()),
            created_at: chrono::Utc::now().timestamp(),
        };
        assert!(crate::api::auth::oauth_owner_is_active(&state, &pending).await);
        sqlx::query("DELETE FROM app_sessions WHERE id = ?")
            .bind(&pending.session_id)
            .execute(&state.pool)
            .await
            .unwrap();
        assert!(!crate::api::auth::oauth_owner_is_active(&state, &pending).await);
    }

    #[tokio::test]
    async fn integration_tokens_are_hash_only_scoped_and_revocable() {
        let state = setup().await;
        seed_user(&state).await;
        let (_, login) = login_as(&state, ClientKind::Bearer).await;
        let user_authorization = format!("Bearer {}", login["token"].as_str().unwrap());

        let (status, _, created) = send(
            crate::api::router(Arc::clone(&state)),
            Method::POST,
            "/integration-tokens",
            Some(serde_json::json!({
                "name": "myAIAgent",
                "scopes": ["mail.accounts.read"]
            })),
            &[("authorization", &user_authorization)],
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        let integration_token = created["token"].as_str().unwrap();
        let token_id = created["id"].as_str().unwrap();
        assert!(integration_token.starts_with("eirs_"));

        let stored: (String, String) =
            sqlx::query_as("SELECT token_hash, token_prefix FROM integration_tokens WHERE id = ?")
                .bind(token_id)
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert_ne!(stored.0, integration_token);
        assert!(integration_token.starts_with(&stored.1));

        let integration_authorization = format!("Bearer {integration_token}");
        let (status, _, _) = send(
            crate::api::router(Arc::clone(&state)),
            Method::GET,
            "/accounts",
            None,
            &[("authorization", &integration_authorization)],
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, _, denied) = send(
            crate::api::router(Arc::clone(&state)),
            Method::GET,
            "/messages/example",
            None,
            &[("authorization", &integration_authorization)],
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(denied["error"].as_str().unwrap().contains("mail.body.read"));

        let (status, _, _) = send(
            crate::api::router(Arc::clone(&state)),
            Method::DELETE,
            &format!("/integration-tokens/{token_id}"),
            None,
            &[("authorization", &user_authorization)],
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, _, _) = send(
            crate::api::router(state),
            Method::GET,
            "/accounts",
            None,
            &[("authorization", &integration_authorization)],
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn connect_scope_lets_an_integration_start_and_complete_provider_sign_in() {
        let state = setup().await;
        seed_user(&state).await;
        let (_, login) = login_as(&state, ClientKind::Bearer).await;
        let user_authorization = format!("Bearer {}", login["token"].as_str().unwrap());

        let mut tokens = Vec::new();
        for scopes in [
            vec!["mail.accounts.read"],
            vec!["mail.accounts.read", "mail.accounts.connect"],
        ] {
            let (status, _, created) = send(
                crate::api::router(Arc::clone(&state)),
                Method::POST,
                "/integration-tokens",
                Some(serde_json::json!({ "name": "myAIAgent", "scopes": scopes })),
                &[("authorization", &user_authorization)],
            )
            .await;
            assert_eq!(status, StatusCode::CREATED, "{created}");
            tokens.push(created);
        }
        let without = format!("Bearer {}", tokens[0]["token"].as_str().unwrap());
        let with = format!("Bearer {}", tokens[1]["token"].as_str().unwrap());

        for (headers, expected) in [
            (vec![], StatusCode::UNAUTHORIZED),
            (
                vec![("authorization", without.as_str())],
                StatusCode::FORBIDDEN,
            ),
            (
                vec![("authorization", with.as_str())],
                StatusCode::BAD_REQUEST,
            ),
        ] {
            let (status, _, _) = send(
                crate::api::router(Arc::clone(&state)),
                Method::POST,
                "/accounts/discover",
                Some(serde_json::json!({"email": "invalid"})),
                &headers,
            )
            .await;
            assert_eq!(status, expected);
        }

        let (status, _, denied) = send(
            crate::api::router(Arc::clone(&state)),
            Method::GET,
            "/auth/gmail/authorize",
            None,
            &[("authorization", &without)],
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(denied["error"]
            .as_str()
            .unwrap()
            .contains("mail.accounts.connect"));

        // With the scope the request passes authentication; the handler then
        // answers from provider configuration, which tests may not have.
        let (status, _, body) = send(
            crate::api::router(Arc::clone(&state)),
            Method::GET,
            "/auth/gmail/authorize",
            None,
            &[("authorization", &with)],
        )
        .await;
        assert_ne!(status, StatusCode::FORBIDDEN, "{body}");
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{body}");

        // The callback accepts a flow started by the scoped token until it is revoked.
        let user_id = login["userId"].as_str().unwrap().to_string();
        let pending = crate::state::OAuthState {
            user_id,
            session_id: tokens[1]["id"].as_str().unwrap().to_string(),
            verifier: None,
            account_id: None,
            created_at: chrono::Utc::now().timestamp(),
        };
        assert!(crate::api::auth::oauth_owner_is_active(&state, &pending).await);
        let unscoped = crate::state::OAuthState {
            session_id: tokens[0]["id"].as_str().unwrap().to_string(),
            ..pending.clone()
        };
        assert!(!crate::api::auth::oauth_owner_is_active(&state, &unscoped).await);
        let (status, _, _) = send(
            crate::api::router(Arc::clone(&state)),
            Method::DELETE,
            &format!("/integration-tokens/{}", tokens[1]["id"].as_str().unwrap()),
            None,
            &[("authorization", &user_authorization)],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(!crate::api::auth::oauth_owner_is_active(&state, &pending).await);
    }
}
