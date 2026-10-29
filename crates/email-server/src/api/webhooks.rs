use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use email_types::{CreateWebhook, UpdateWebhook, Webhook as WebhookRow};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::state::AppState;

pub async fn list_webhooks(State(state): State<Arc<AppState>>) -> Result<Json<Vec<WebhookRow>>> {
    let rows = sqlx::query_as::<_, WebhookRow>(
        "SELECT id, url, secret, events, account_id, enabled, created_at FROM webhooks ORDER BY created_at",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

pub async fn create_webhook(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWebhook>,
) -> Result<Json<WebhookRow>> {
    let id = Uuid::new_v4().to_string();
    let events = body.events.unwrap_or_else(|| "message.created".to_string());
    validate_events(&events)?;
    validate_webhook_url(&body.url, &state.webhook_policy)?;

    sqlx::query(
        r#"INSERT INTO webhooks (id, url, secret, events, account_id)
           VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&body.url)
    .bind(&body.secret)
    .bind(&events)
    .bind(&body.account_id)
    .execute(&state.pool)
    .await?;

    let row = sqlx::query_as::<_, WebhookRow>(
        "SELECT id, url, secret, events, account_id, enabled, created_at FROM webhooks WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

pub async fn update_webhook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateWebhook>,
) -> Result<Json<WebhookRow>> {
    if let Some(ref url) = body.url {
        validate_webhook_url(url, &state.webhook_policy)?;
        sqlx::query("UPDATE webhooks SET url = ? WHERE id = ?")
            .bind(url)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(ref secret) = body.secret {
        sqlx::query("UPDATE webhooks SET secret = ? WHERE id = ?")
            .bind(secret)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(ref events) = body.events {
        validate_events(events)?;
        sqlx::query("UPDATE webhooks SET events = ? WHERE id = ?")
            .bind(events)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(enabled) = body.enabled {
        sqlx::query("UPDATE webhooks SET enabled = ? WHERE id = ?")
            .bind(enabled)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    let row = sqlx::query_as::<_, WebhookRow>(
        "SELECT id, url, secret, events, account_id, enabled, created_at FROM webhooks WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("webhook {} not found", id)))?;

    Ok(Json(row))
}

pub async fn delete_webhook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let rows = sqlx::query("DELETE FROM webhooks WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("webhook {} not found", id)));
    }
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

const MAX_DELIVERY_ATTEMPTS: i64 = 8;

#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationEvent {
    pub id: String,
    pub event_type: String,
    pub account_id: String,
    #[sqlx(json)]
    pub data: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookDelivery {
    pub event_id: String,
    pub webhook_id: String,
    pub status: String,
    pub attempts: i64,
    pub next_attempt_at: String,
    pub last_attempt_at: Option<String>,
    pub response_status: Option<i64>,
    pub last_error: Option<String>,
    pub delivered_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EventQuery {
    pub after: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebhookPayload<'a> {
    id: &'a str,
    event: &'a str,
    timestamp: &'a str,
    account_id: &'a str,
    data: &'a serde_json::Value,
}

#[derive(Debug, sqlx::FromRow)]
struct PendingDelivery {
    event_id: String,
    webhook_id: String,
    event_type: String,
    account_id: String,
    data_json: String,
    created_at: String,
    url: String,
    secret: Option<String>,
    attempts: i64,
}

pub async fn list_events(
    State(state): State<Arc<AppState>>,
    Query(query): Query<EventQuery>,
) -> Result<Json<Vec<IntegrationEvent>>> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let events = if let Some(after) = query.after {
        sqlx::query_as::<_, IntegrationEvent>(
            r#"SELECT id, event_type, account_id, data_json AS data, created_at
               FROM webhook_events
               WHERE (created_at, id) > (
                   SELECT created_at, id FROM webhook_events WHERE id = ?
               )
               ORDER BY created_at, id LIMIT ?"#,
        )
        .bind(after)
        .bind(limit)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, IntegrationEvent>(
            r#"SELECT id, event_type, account_id, data_json AS data, created_at
               FROM webhook_events ORDER BY created_at, id LIMIT ?"#,
        )
        .bind(limit)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(events))
}

pub async fn list_deliveries(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<WebhookDelivery>>> {
    let rows = sqlx::query_as::<_, WebhookDelivery>(
        r#"SELECT event_id, webhook_id, status, attempts, next_attempt_at,
                  last_attempt_at, response_status, last_error, delivered_at
           FROM webhook_deliveries ORDER BY next_attempt_at DESC LIMIT 500"#,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

/// Event names a webhook subscription may list, besides the `all` / `*` wildcards.
const KNOWN_EVENTS: &[&str] = &[
    "message.created",
    "message.updated",
    "message.moved",
    "message.deleted",
    "sync.completed",
    "new_message",
];

fn event_matches(configured: &str, event: &str) -> bool {
    configured.split(',').map(str::trim).any(|candidate| {
        candidate == "all"
            || candidate == "*"
            || candidate == event
            || (candidate == "new_message" && event == "message.created")
    })
}

/// Reject subscriptions that could never match an event, so a typo cannot
/// silently disable delivery.
fn validate_events(configured: &str) -> Result<()> {
    let unknown: Vec<&str> = configured
        .split(',')
        .map(str::trim)
        .filter(|c| !(c.is_empty() || *c == "all" || *c == "*" || KNOWN_EVENTS.contains(c)))
        .collect();
    if !unknown.is_empty() {
        return Err(AppError::Validation(format!(
            "unknown webhook event(s): {}; expected all, *, or one of {}",
            unknown.join(", "),
            KNOWN_EVENTS.join(", ")
        )));
    }
    if configured.split(',').all(|c| c.trim().is_empty()) {
        return Err(AppError::Validation(
            "webhook events must not be empty".into(),
        ));
    }
    Ok(())
}

/// Persist an event and enqueue all matching deliveries before returning.
pub async fn fire_webhooks(
    pool: &sqlx::SqlitePool,
    account_id: &str,
    event: &str,
    mut data: serde_json::Value,
) {
    let event_id = data
        .get("eventId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if let Some(object) = data.as_object_mut() {
        object.remove("eventId");
    }
    let data_json = match serde_json::to_string(&data) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, "webhook event serialization failed");
            return;
        }
    };
    let rows = match sqlx::query_as::<_, WebhookRow>(
        r#"SELECT id, url, secret, events, account_id, enabled, created_at
           FROM webhooks
           WHERE enabled = 1
             AND (account_id IS NULL OR account_id = ?)"#,
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("failed to query webhooks: {}", e);
            return;
        }
    };

    // BEGIN IMMEDIATE: a deferred insert that has already read returns
    // SQLITE_BUSY immediately and ignores busy_timeout.
    let mut tx = match crate::db::begin_write(pool).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::warn!(%error, "webhook event transaction failed");
            return;
        }
    };
    if let Err(error) = sqlx::query(
        r#"INSERT OR IGNORE INTO webhook_events (id, event_type, account_id, data_json)
           VALUES (?, ?, ?, ?)"#,
    )
    .bind(&event_id)
    .bind(event)
    .bind(account_id)
    .bind(&data_json)
    .execute(&mut *tx)
    .await
    {
        tracing::warn!(%error, "failed to persist webhook event");
        return;
    }

    for webhook in rows
        .into_iter()
        .filter(|webhook| event_matches(&webhook.events, event))
    {
        if let Err(error) = sqlx::query(
            r#"INSERT OR IGNORE INTO webhook_deliveries (event_id, webhook_id)
               VALUES (?, ?)"#,
        )
        .bind(&event_id)
        .bind(&webhook.id)
        .execute(&mut *tx)
        .await
        {
            tracing::warn!(%error, "failed to enqueue webhook delivery");
        }
    }
    if let Err(error) = tx.commit().await {
        tracing::warn!(%error, "webhook event commit failed");
    }
}

/// HMAC-SHA256 over `{timestamp}.{body}`, so a receiver can reject a replay
/// of an old body by checking the timestamp window before verifying the MAC.
pub fn signature(timestamp: &str, body: &str, secret: &str) -> String {
    use std::fmt::Write as _;
    let mut message = Vec::with_capacity(timestamp.len() + 1 + body.len());
    message.extend_from_slice(timestamp.as_bytes());
    message.push(b'.');
    message.extend_from_slice(body.as_bytes());
    let key = hmac_sha256::HMAC::mac(&message, secret.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in key {
        let _ = write!(hex, "{byte:02x}");
    }
    format!("sha256={hex}")
}

/// True when `presented` is the signature of `timestamp` and `body`.
pub fn signature_matches(timestamp: &str, body: &str, secret: &str, presented: &str) -> bool {
    let expected = signature(timestamp, body, secret);
    // Constant-time compare of equal-length hex strings.
    expected.len() == presented.len()
        && expected
            .bytes()
            .zip(presented.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

#[derive(Debug)]
struct VettedUrl {
    host: String,
    socket: std::net::SocketAddr,
}

fn validate_webhook_url(url: &str, policy: &crate::config::WebhookPolicy) -> Result<()> {
    parse_webhook_url(url, policy).map(|_| ())
}

fn parse_webhook_url(
    url: &str,
    policy: &crate::config::WebhookPolicy,
) -> Result<(String, u16, bool)> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|_| AppError::Validation("webhook url is not a valid absolute URL".into()))?;
    if parsed.username() != "" || parsed.password().is_some() {
        return Err(AppError::Validation(
            "webhook url must not contain credentials".into(),
        ));
    }
    let host = parsed
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| AppError::Validation("webhook url has no host".into()))?
        .to_string();
    let https = parsed.scheme() == "https";
    let http = parsed.scheme() == "http";
    if https {
        // always allowed
    } else if http && policy.allow_insecure_http {
        // explicit development opt-in
    } else {
        return Err(AppError::Validation(
            "webhook url must be https (http only when EMAIL_RS_ALLOW_INSECURE_HTTP is set)".into(),
        ));
    }
    if policy.allowlist.iter().any(|allowed| allowed == url) {
        let port = parsed
            .port_or_known_default()
            .unwrap_or(if https { 443 } else { 80 });
        return Ok((host, port, true));
    }
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| AppError::Validation("webhook url has no port".into()))?;
    Ok((host, port, false))
}

/// IPv4 carried inside a transition address: NAT64 (`64:ff9b::/96` and
/// `64:ff9b:1::/48`), IPv4-compatible (`::/96`, excluding unspecified and
/// loopback which are already blocked as IPv6), 6to4 (`2002::/16`) and
/// Teredo (`2001::/32`). IPv4-mapped (`::ffff:0:0/96`) is handled separately.
fn embedded_ipv4(v6: std::net::Ipv6Addr) -> Option<std::net::Ipv4Addr> {
    let s = v6.segments();
    let tail =
        std::net::Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8);
    // NAT64 well-known prefix 64:ff9b::/96.
    if s[0] == 0x0064 && s[1] == 0xff9b && s[2] == 0 && s[3] == 0 && s[4] == 0 && s[5] == 0 {
        return Some(tail);
    }
    // NAT64 local-use prefix 64:ff9b:1::/48.
    if s[0] == 0x0064 && s[1] == 0xff9b && s[2] == 0x0001 {
        return Some(tail);
    }
    // IPv4-compatible ::/96. :: and ::1 are not transition addresses.
    if s[0] == 0
        && s[1] == 0
        && s[2] == 0
        && s[3] == 0
        && s[4] == 0
        && s[5] == 0
        && !tail.is_unspecified()
        && !tail.is_loopback()
    {
        return Some(tail);
    }
    // 6to4 2002::/16: the IPv4 is the next 32 bits.
    if s[0] == 0x2002 {
        return Some(std::net::Ipv4Addr::new(
            (s[1] >> 8) as u8,
            s[1] as u8,
            (s[2] >> 8) as u8,
            s[2] as u8,
        ));
    }
    // Teredo 2001:0::/32: the client IPv4 is the last 32 bits, inverted.
    if s[0] == 0x2001 && s[1] == 0 {
        return Some(std::net::Ipv4Addr::new(
            !(s[6] >> 8) as u8,
            !s[6] as u8,
            !(s[7] >> 8) as u8,
            !s[7] as u8,
        ));
    }
    None
}

fn ip_is_blocked(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                // CGNAT (Tailscale and similar overlays sit in 100.64.0.0/10).
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64)
                // Cloud metadata (169.254.169.254 is link-local; also block the
                // whole 169.254.0.0/16 via is_link_local) and 0.0.0.0/8.
                || v4.octets()[0] == 0
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 // fc00::/7 unique local
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // fe80::/10 link-local
                || (v6.segments()[0] & 0xffc0) == 0xfec0 // fec0::/10 deprecated site-local
                // IPv4-mapped addresses must be judged as IPv4.
                || v6.to_ipv4_mapped().is_some_and(|v4| ip_is_blocked(v4.into()))
                // Transition forms that carry an embedded IPv4: judge that IPv4.
                || embedded_ipv4(v6).is_some_and(|v4| ip_is_blocked(v4.into()))
                // Documentation range (RFC 3849).
                || (v6.segments()[0] == 0x2001 && v6.segments()[1] == 0x0db8)
        }
    }
}

async fn vet_delivery_url(
    url: &str,
    policy: &crate::config::WebhookPolicy,
) -> std::result::Result<VettedUrl, String> {
    let (host, port, allowlisted) = parse_webhook_url(url, policy).map_err(|e| e.to_string())?;
    if allowlisted {
        let socket = resolve_first(&host, port)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(VettedUrl { host, socket });
    }
    let addresses = lookup_host(&host, port).await.map_err(|e| e.to_string())?;
    if addresses.is_empty() {
        return Err(format!("webhook host {host} did not resolve"));
    }
    if let Some(blocked) = addresses.iter().find(|addr| ip_is_blocked(addr.ip())) {
        return Err(format!(
            "webhook target {blocked} is a private, loopback, link-local or metadata address"
        ));
    }
    Ok(VettedUrl {
        host,
        socket: addresses[0],
    })
}

async fn lookup_host(host: &str, port: u16) -> Result<Vec<std::net::SocketAddr>> {
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return Ok(vec![std::net::SocketAddr::new(ip, port)]);
    }
    tokio::net::lookup_host((host, port))
        .await
        .map(|iter| iter.collect())
        .map_err(|error| AppError::Validation(format!("webhook host did not resolve: {error}")))
}

async fn resolve_first(host: &str, port: u16) -> Result<std::net::SocketAddr> {
    lookup_host(host, port)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Validation(format!("webhook host {host} did not resolve")))
}

fn delivery_client(vetted: &VettedUrl) -> std::result::Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        // Environment proxies would connect somewhere other than the pinned socket.
        .no_proxy()
        .resolve(&vetted.host, vetted.socket)
        .build()
}

async fn record_failure(
    pool: &sqlx::SqlitePool,
    row: &PendingDelivery,
    attempt: i64,
    response_status: Option<i64>,
    error: String,
) {
    let dead = attempt >= MAX_DELIVERY_ATTEMPTS;
    let delay = (5_i64 * 2_i64.pow((attempt.min(7) - 1) as u32)).min(300);
    let modifier = format!("+{delay} seconds");
    let _ = sqlx::query(
        r#"UPDATE webhook_deliveries
           SET status = ?, attempts = ?, response_status = ?, last_error = ?,
               next_attempt_at = datetime('now', ?)
           WHERE event_id = ? AND webhook_id = ?"#,
    )
    .bind(if dead { "dead_letter" } else { "retry" })
    .bind(attempt)
    .bind(response_status)
    .bind(error.chars().take(2000).collect::<String>())
    .bind(modifier)
    .bind(&row.event_id)
    .bind(&row.webhook_id)
    .execute(pool)
    .await;
}

async fn deliver_one(
    pool: &sqlx::SqlitePool,
    policy: &crate::config::WebhookPolicy,
    row: PendingDelivery,
) {
    let claimed = sqlx::query(
        r#"UPDATE webhook_deliveries
           SET status = 'delivering', last_attempt_at = datetime('now')
           WHERE event_id = ? AND webhook_id = ?
             AND status IN ('pending', 'retry') AND next_attempt_at <= datetime('now')"#,
    )
    .bind(&row.event_id)
    .bind(&row.webhook_id)
    .execute(pool)
    .await;
    if !matches!(claimed, Ok(result) if result.rows_affected() == 1) {
        return;
    }

    let data = match serde_json::from_str(&row.data_json) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, event_id = %row.event_id, "stored webhook JSON is invalid");
            return;
        }
    };
    let payload = WebhookPayload {
        id: &row.event_id,
        event: &row.event_type,
        timestamp: &row.created_at,
        account_id: &row.account_id,
        data: &data,
    };
    let body = match serde_json::to_string(&payload) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, "webhook payload serialization failed");
            return;
        }
    };
    let vetted = match vet_delivery_url(&row.url, policy).await {
        Ok(vetted) => vetted,
        Err(error) => {
            record_failure(pool, &row, row.attempts + 1, None, error).await;
            return;
        }
    };
    let client = match delivery_client(&vetted) {
        Ok(client) => client,
        Err(error) => {
            tracing::warn!(%error, "webhook client creation failed");
            return;
        }
    };
    let sent_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string());
    let mut request = client
        .post(&row.url)
        .header("Content-Type", "application/json")
        .header("X-Email-Event", &row.event_type)
        .header("X-Email-Event-ID", &row.event_id)
        .header("X-Email-Timestamp", &sent_at)
        .body(body.clone());
    if let Some(secret) = &row.secret {
        // Legacy body-only MAC, kept so existing receivers keep verifying.
        // Deprecated: new receivers should use the v2 headers below.
        let legacy = {
            let key = hmac_sha256::HMAC::mac(body.as_bytes(), secret.as_bytes());
            let mut hex = String::with_capacity(64);
            for byte in key {
                use std::fmt::Write as _;
                let _ = write!(hex, "{byte:02x}");
            }
            format!("sha256={hex}")
        };
        request = request
            .header("X-Email-Signature", legacy)
            .header("X-EmailRs-Timestamp", &sent_at)
            .header("X-EmailRs-Signature-V2", signature(&sent_at, &body, secret));
    }

    let attempt = row.attempts + 1;
    match request.send().await {
        Ok(response) if response.status().is_success() => {
            let _ = sqlx::query(
                r#"UPDATE webhook_deliveries
                   SET status = 'delivered', attempts = ?, response_status = ?,
                       last_error = NULL, delivered_at = datetime('now')
                   WHERE event_id = ? AND webhook_id = ?"#,
            )
            .bind(attempt)
            .bind(i64::from(response.status().as_u16()))
            .bind(&row.event_id)
            .bind(&row.webhook_id)
            .execute(pool)
            .await;
        }
        result => {
            let (response_status, error) = match result {
                Ok(response) => (
                    Some(i64::from(response.status().as_u16())),
                    format!("HTTP {}", response.status()),
                ),
                Err(error) => (None, error.to_string()),
            };
            let dead = attempt >= MAX_DELIVERY_ATTEMPTS;
            let delay = (5_i64 * 2_i64.pow((attempt.min(7) - 1) as u32)).min(300);
            let modifier = format!("+{delay} seconds");
            let _ = sqlx::query(
                r#"UPDATE webhook_deliveries
                   SET status = ?, attempts = ?, response_status = ?, last_error = ?,
                       next_attempt_at = datetime('now', ?)
                   WHERE event_id = ? AND webhook_id = ?"#,
            )
            .bind(if dead { "dead_letter" } else { "retry" })
            .bind(attempt)
            .bind(response_status)
            .bind(error.chars().take(2000).collect::<String>())
            .bind(modifier)
            .bind(&row.event_id)
            .bind(&row.webhook_id)
            .execute(pool)
            .await;
        }
    }
}

pub async fn dispatch_due(
    pool: &sqlx::SqlitePool,
    policy: &crate::config::WebhookPolicy,
) -> Result<usize> {
    let rows = sqlx::query_as::<_, PendingDelivery>(
        r#"SELECT d.event_id, d.webhook_id, e.event_type, e.account_id,
                  e.data_json, e.created_at, w.url, w.secret, d.attempts
           FROM webhook_deliveries d
           JOIN webhook_events e ON e.id = d.event_id
           JOIN webhooks w ON w.id = d.webhook_id
           WHERE d.status IN ('pending', 'retry')
             AND d.next_attempt_at <= datetime('now') AND w.enabled = 1
           ORDER BY d.next_attempt_at, e.created_at LIMIT 50"#,
    )
    .fetch_all(pool)
    .await?;
    let count = rows.len();
    for row in rows {
        deliver_one(pool, policy, row).await;
    }
    Ok(count)
}

pub fn spawn_delivery_worker(pool: sqlx::SqlitePool, policy: crate::config::WebhookPolicy) {
    tokio::spawn(async move {
        let _ = sqlx::query(
            "UPDATE webhook_deliveries SET status = 'retry' WHERE status = 'delivering'",
        )
        .execute(&pool)
        .await;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            if let Err(error) = dispatch_due(&pool, &policy).await {
                tracing::warn!(%error, "webhook delivery worker failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use axum::{http::StatusCode, routing::post, Router};

    use super::*;

    async fn setup() -> sqlx::SqlitePool {
        let path = format!("/tmp/email_webhook_test_{}.db", Uuid::new_v4());
        crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap()
            .0
    }

    async fn webhook(pool: &sqlx::SqlitePool, url: &str, secret: Option<&str>) {
        sqlx::query(
            r#"INSERT INTO webhooks (id, url, secret, events)
               VALUES ('hook-1', ?, ?, 'new_message,message.updated')"#,
        )
        .bind(url)
        .bind(secret)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn server(status: StatusCode) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/", post(move || async move { status })),
            )
            .await
            .unwrap();
        });
        format!("http://{address}/")
    }

    fn dev_policy(allowlist: &[&str]) -> crate::config::WebhookPolicy {
        crate::config::WebhookPolicy {
            allow_insecure_http: true,
            allowlist: allowlist.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    fn locked_policy() -> crate::config::WebhookPolicy {
        crate::config::WebhookPolicy {
            allow_insecure_http: false,
            allowlist: vec![],
        }
    }

    #[tokio::test]
    async fn event_and_matching_delivery_are_persisted_before_dispatch() {
        let pool = setup().await;
        webhook(&pool, "http://127.0.0.1:1/", Some("secret")).await;

        fire_webhooks(
            &pool,
            "account-1",
            "message.created",
            serde_json::json!({
                "eventId": "event-1",
                "logicalId": "logical-1",
                "occurrenceId": "occurrence-1"
            }),
        )
        .await;

        let event: (String, String, String) = sqlx::query_as(
            "SELECT event_type, account_id, data_json FROM webhook_events WHERE id = 'event-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let delivery: (String, i64) = sqlx::query_as(
            "SELECT status, attempts FROM webhook_deliveries WHERE event_id = 'event-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(event.0, "message.created");
        assert_eq!(event.1, "account-1");
        assert!(!event.2.contains("eventId"));
        assert_eq!(delivery, ("pending".to_string(), 0));
    }

    #[tokio::test]
    async fn dispatcher_records_success_and_retryable_failure() {
        let success_pool = setup().await;
        let success_url = server(StatusCode::NO_CONTENT).await;
        webhook(&success_pool, &success_url, None).await;
        fire_webhooks(
            &success_pool,
            "account-1",
            "message.updated",
            serde_json::json!({"eventId": "success-1"}),
        )
        .await;
        assert_eq!(
            dispatch_due(&success_pool, &dev_policy(&[&success_url]))
                .await
                .unwrap(),
            1
        );
        let success: (String, i64, i64) = sqlx::query_as(
            "SELECT status, attempts, response_status FROM webhook_deliveries WHERE event_id = 'success-1'",
        )
        .fetch_one(&success_pool)
        .await
        .unwrap();
        assert_eq!(success, ("delivered".to_string(), 1, 204));

        let failure_pool = setup().await;
        let failure_url = server(StatusCode::INTERNAL_SERVER_ERROR).await;
        webhook(&failure_pool, &failure_url, None).await;
        fire_webhooks(
            &failure_pool,
            "account-1",
            "message.updated",
            serde_json::json!({"eventId": "failure-1"}),
        )
        .await;
        assert_eq!(
            dispatch_due(&failure_pool, &dev_policy(&[&failure_url]))
                .await
                .unwrap(),
            1
        );
        let failure: (String, i64, i64) = sqlx::query_as(
            "SELECT status, attempts, response_status FROM webhook_deliveries WHERE event_id = 'failure-1'",
        )
        .fetch_one(&failure_pool)
        .await
        .unwrap();
        assert_eq!(failure, ("retry".to_string(), 1, 500));
    }

    #[test]
    fn private_loopback_and_metadata_literals_are_blocked() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.1.9",
            "172.16.0.1",
            "169.254.169.254",
            "169.254.1.1",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
        ] {
            let parsed: std::net::IpAddr = ip.parse().unwrap();
            assert!(ip_is_blocked(parsed), "{ip} should be blocked");
        }
        for ip in ["1.1.1.1", "8.8.8.8", "2001:4860:4860::8888"] {
            let parsed: std::net::IpAddr = ip.parse().unwrap();
            assert!(!ip_is_blocked(parsed), "{ip} should be public");
        }
    }

    #[test]
    fn webhook_urls_must_be_https_unless_the_dev_flag_is_set() {
        let locked = crate::config::WebhookPolicy {
            allow_insecure_http: false,
            allowlist: vec![],
        };
        let open = crate::config::WebhookPolicy {
            allow_insecure_http: true,
            allowlist: vec![],
        };
        assert!(parse_webhook_url("https://hooks.example.com/email", &locked).is_ok());
        assert!(parse_webhook_url("http://hooks.example.com/email", &locked).is_err());
        assert!(parse_webhook_url("http://hooks.example.com/email", &open).is_ok());
        assert!(parse_webhook_url("ftp://hooks.example.com/email", &open).is_err());
        assert!(parse_webhook_url("https://user:pass@hooks.example.com/", &locked).is_err());
    }

    #[tokio::test]
    async fn delivery_refuses_a_loopback_target_that_is_not_allowlisted() {
        let err = vet_delivery_url("http://127.0.0.1:9/hook", &locked_policy())
            .await
            .unwrap_err();
        assert!(
            err.contains("private") || err.contains("https") || err.contains("loopback"),
            "{err}"
        );
        let literal = vet_delivery_url("https://127.0.0.1/hook", &locked_policy())
            .await
            .unwrap_err();
        assert!(
            literal.contains("private") || literal.contains("loopback"),
            "{literal}"
        );
        let metadata = vet_delivery_url("https://169.254.169.254/latest", &locked_policy())
            .await
            .unwrap_err();
        assert!(
            metadata.contains("private")
                || metadata.contains("metadata")
                || metadata.contains("link-local"),
            "{metadata}"
        );
    }

    #[tokio::test]
    async fn delivery_records_a_redirect_as_failure_without_following_it() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/",
                    post(|| async {
                        (
                            StatusCode::FOUND,
                            [("Location", "http://169.254.169.254/latest")],
                            "nope",
                        )
                    }),
                ),
            )
            .await
            .unwrap();
        });
        let url = format!("http://{address}/");
        let pool = setup().await;
        webhook(&pool, &url, Some("topsecret")).await;
        fire_webhooks(
            &pool,
            "account-1",
            "message.updated",
            serde_json::json!({"eventId": "redir-1"}),
        )
        .await;
        assert_eq!(dispatch_due(&pool, &dev_policy(&[&url])).await.unwrap(), 1);
        let status: (String, i64) = sqlx::query_as(
            "SELECT status, response_status FROM webhook_deliveries WHERE event_id = 'redir-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        // Redirects are not followed, so a 302 is a failed delivery, not a call
        // to the metadata address in Location.
        assert_eq!(status.0, "retry");
        assert_eq!(status.1, 302);
    }

    #[test]
    fn timestamped_signature_covers_timestamp_and_body_and_rejects_replay() {
        let body = r#"{"id":"event-1"}"#;
        let signed = signature("1700000000", body, "topsecret");
        assert!(signature_matches("1700000000", body, "topsecret", &signed));
        // A body-only MAC of the same bytes does not verify.
        let body_only = {
            let key = hmac_sha256::HMAC::mac(body.as_bytes(), b"topsecret");
            key.iter().map(|b| format!("{b:02x}")).collect::<String>()
        };
        assert!(!signature_matches(
            "1700000000",
            body,
            "topsecret",
            &format!("sha256={body_only}")
        ));
        // Replaying the same body under a different timestamp fails.
        assert!(!signature_matches("1700000099", body, "topsecret", &signed));
        assert!(!signature_matches("1700000000", body, "other", &signed));
        assert!(signed.starts_with("sha256="));
    }

    #[test]
    fn wildcard_subscriptions_match_every_event() {
        for configured in ["*", "all", " * "] {
            assert!(event_matches(configured, "message.created"));
            assert!(event_matches(configured, "sync.completed"));
        }
        assert!(event_matches("new_message", "message.created"));
        assert!(!event_matches("message.updated", "message.created"));
    }

    #[test]
    fn unknown_event_patterns_are_rejected() {
        assert!(validate_events("*").is_ok());
        assert!(validate_events("all").is_ok());
        assert!(validate_events("new_message, message.updated").is_ok());
        assert!(validate_events("message.*").is_err());
        assert!(validate_events("message.create").is_err());
        assert!(validate_events(" , ").is_err());
    }

    #[tokio::test]
    async fn star_subscription_enqueues_delivery() {
        let pool = setup().await;
        sqlx::query(
            "INSERT INTO webhooks (id, url, secret, events) VALUES ('hook-star', 'http://127.0.0.1:1/', NULL, '*')",
        )
        .execute(&pool)
        .await
        .unwrap();
        fire_webhooks(
            &pool,
            "account-1",
            "message.created",
            serde_json::json!({"eventId": "star-1"}),
        )
        .await;
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM webhook_deliveries WHERE event_id = 'star-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn transition_addresses_are_judged_by_their_embedded_ipv4() {
        for ip in [
            "64:ff9b::101:101",
            "64:ff9b:1::808:808",
            "::808:808",
            "2002:0808:0808::",
            // Teredo client IPv4 is the last 32 bits inverted; 8.8.8.8 inverted
            // is f7f7:f7f7.
            "2001:0:1:2:3:4:f7f7:f7f7",
            "2001:4860:4860::8888",
        ] {
            let parsed: std::net::IpAddr = ip.parse().unwrap();
            assert!(!ip_is_blocked(parsed), "{ip} should be public");
        }
        // NAT64 well-known prefix carrying 10.0.0.1.
        assert!(ip_is_blocked("64:ff9b::a00:1".parse().unwrap()));
        // NAT64 local-use prefix carrying 192.168.1.1.
        assert!(ip_is_blocked("64:ff9b:1::c0a8:101".parse().unwrap()));
        // IPv4-compatible ::/96 carrying 10.0.0.1 (not the mapped form ::ffff:0:a00:1).
        assert!(ip_is_blocked("::a00:1".parse().unwrap()));
        // 6to4 carrying 10.1.2.3.
        assert!(ip_is_blocked("2002:0a01:0203::".parse().unwrap()));
        // Teredo carrying 10.0.0.1 (inverted f5ff:fffe).
        assert!(ip_is_blocked("2001:0:1:2:3:4:f5ff:fffe".parse().unwrap()));
        // Documentation range.
        assert!(ip_is_blocked("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn delivery_client_ignores_environment_proxies() {
        let vetted = VettedUrl {
            host: "hooks.example.com".into(),
            socket: "1.1.1.1:443".parse().unwrap(),
        };
        // A proxy env var must not make the pinned client fail to build, and
        // the builder used by delivery_client sets no_proxy so HTTP(S)_PROXY
        // and ALL_PROXY cannot redirect the connection off the pinned socket.
        let client = delivery_client(&vetted).expect("client builds");
        drop(client);
    }

    #[test]
    fn legacy_signature_is_the_body_only_mac() {
        let body = r#"{"id":"event-1"}"#;
        let key = hmac_sha256::HMAC::mac(body.as_bytes(), b"topsecret");
        let legacy = format!(
            "sha256={}",
            key.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
        let v2 = signature("1700000000", body, "topsecret");
        assert_ne!(legacy, v2, "v2 must not replace the legacy body-only MAC");
    }
}
