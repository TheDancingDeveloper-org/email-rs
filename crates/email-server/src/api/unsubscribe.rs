//! `List-Unsubscribe` (RFC 2369) parsing and the unsubscribe action, with
//! RFC 8058 one-click POST and a `mailto:` fallback queued through the outbox.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    extract::{Path, State},
    Json,
};
use email_types::{ListUnsubscribe, SendRequest};
use serde::Serialize;

use super::{
    messages::{header_value, unfolded_headers},
    outbox,
    webhooks::ip_is_blocked,
};
use crate::{
    error::{AppError, Result},
    state::AppState,
};

const ONE_CLICK_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_URL_BYTES: usize = 2048;

#[derive(Debug, Serialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnsubscribeResponse {
    /// `one_click` or `mailto`.
    pub method: String,
    /// HTTP status of the one-click POST, absent for `mailto`.
    pub http_status: Option<u16>,
    /// Outbox entry queued for `mailto`, absent for `one_click`.
    pub outbox: Option<serde_json::Value>,
}

/// Targets advertised by the raw header block, or `None` when the message
/// offers no usable unsubscribe target.
pub(crate) fn parse_list_unsubscribe(raw_headers: &str) -> Option<ListUnsubscribe> {
    let headers = unfolded_headers(raw_headers);
    let value = header_value(&headers, "List-Unsubscribe")?;
    let mut parsed = ListUnsubscribe::default();
    let mut rest = value;
    while let Some(start) = rest.find('<') {
        let Some(len) = rest[start + 1..].find('>') else {
            break;
        };
        let target = rest[start + 1..start + 1 + len].trim();
        rest = &rest[start + 1 + len + 1..];
        if target.is_empty()
            || target.len() > MAX_URL_BYTES
            || target.chars().any(|c| c.is_control() || c.is_whitespace())
        {
            continue;
        }
        let lower = target.to_ascii_lowercase();
        if lower.starts_with("https://") {
            parsed.https.push(target.to_string());
        } else if lower.starts_with("mailto:") {
            parsed.mailto.push(target.to_string());
        }
    }
    parsed.one_click = !parsed.https.is_empty()
        && header_value(&headers, "List-Unsubscribe-Post").is_some_and(|post| {
            post.split(';').any(|part| {
                part.trim()
                    .eq_ignore_ascii_case("List-Unsubscribe=One-Click")
            })
        });
    (!parsed.https.is_empty() || !parsed.mailto.is_empty()).then_some(parsed)
}

pub async fn unsubscribe_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<String>,
) -> Result<Json<UnsubscribeResponse>> {
    let (account_id, raw_headers) = sqlx::query_as::<_, (String, Option<String>)>(
        r#"SELECT m.account_id, b.raw_headers FROM messages m
           LEFT JOIN message_bodies b ON b.message_id = m.id WHERE m.id = ?"#,
    )
    .bind(&message_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("message {message_id} not found")))?;
    let targets = raw_headers
        .as_deref()
        .and_then(parse_list_unsubscribe)
        .ok_or_else(|| AppError::Validation("message offers no unsubscribe target".into()))?;

    if targets.one_click {
        let mut last_error = None;
        for url in &targets.https {
            match post_one_click(url, false).await {
                Ok(status) => {
                    return Ok(Json(UnsubscribeResponse {
                        method: "one_click".into(),
                        http_status: Some(status),
                        outbox: None,
                    }))
                }
                Err(error) => last_error = Some(error),
            }
        }
        if targets.mailto.is_empty() {
            return Err(last_error
                .unwrap_or_else(|| AppError::Provider("unsubscribe request failed".into())));
        }
    }

    let Some(mailto) = targets.mailto.first() else {
        return Err(AppError::Validation(
            "message only offers an HTTPS unsubscribe link without one-click support; \
             open it in a browser"
                .into(),
        ));
    };
    let (to, subject, body) = parse_mailto(mailto)?;
    let request = SendRequest {
        account_id,
        to: vec![to],
        cc: vec![],
        bcc: vec![],
        subject,
        text_body: Some(body),
        html_body: None,
        in_reply_to: None,
        attachments: vec![],
        send_at: None,
        send_delay_seconds: None,
        identity_id: None,
    };
    let queued = outbox::enqueue(&state, request).await?;
    Ok(Json(UnsubscribeResponse {
        method: "mailto".into(),
        http_status: None,
        outbox: Some(
            serde_json::to_value(queued).map_err(|e| AppError::Validation(e.to_string()))?,
        ),
    }))
}

/// Recipient, subject and body of a `mailto:` unsubscribe target.
fn parse_mailto(target: &str) -> Result<(String, String, String)> {
    let invalid = || AppError::Validation("unsubscribe mailto target is invalid".into());
    let rest = target.get("mailto:".len()..).ok_or_else(invalid)?;
    let (address, query) = rest.split_once('?').unwrap_or((rest, ""));
    let address = urlencoding::decode(address)
        .map_err(|_| invalid())?
        .into_owned();
    // A single plain address: no lists, display names or header injection.
    if address.is_empty()
        || address.contains([',', ';', '<', '>', '"', ' '])
        || address.chars().any(|c| c.is_control())
        || address.matches('@').count() != 1
        || address.starts_with('@')
        || address.ends_with('@')
    {
        return Err(invalid());
    }
    let (mut subject, mut body) = (String::new(), String::new());
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let value = urlencoding::decode(&value.replace('+', " "))
            .map_err(|_| invalid())?
            .into_owned();
        if value
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\r')
        {
            return Err(invalid());
        }
        match key.to_ascii_lowercase().as_str() {
            "subject" if !value.contains(['\r', '\n']) => subject = value,
            "subject" => return Err(invalid()),
            "body" => body = value,
            _ => {}
        }
    }
    if subject.is_empty() {
        subject = "unsubscribe".into();
    }
    if body.is_empty() {
        body = "unsubscribe".into();
    }
    Ok((address, subject, body))
}

/// RFC 8058 one-click POST. The host is resolved once, every address is
/// vetted, and the connection is pinned to the vetted socket. Redirects are
/// never followed, so a 3xx answer is a failure rather than a request to the
/// `Location`. `relaxed` (tests only) permits plain HTTP and local addresses.
async fn post_one_click(url: &str, relaxed: bool) -> Result<u16> {
    let invalid = |msg: &str| AppError::Validation(format!("unsubscribe url {msg}"));
    let parsed = reqwest::Url::parse(url).map_err(|_| invalid("is not a valid URL"))?;
    if parsed.scheme() != "https" && !(relaxed && parsed.scheme() == "http") {
        return Err(invalid("must be https"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(invalid("must not contain credentials"));
    }
    let host = parsed
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| invalid("has no host"))?
        .to_string();
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| invalid("has no port"))?;
    let addresses: Vec<SocketAddr> = match host.trim_matches(['[', ']']).parse::<std::net::IpAddr>()
    {
        Ok(ip) => vec![SocketAddr::new(ip, port)],
        Err(_) => tokio::time::timeout(
            ONE_CLICK_TIMEOUT,
            tokio::net::lookup_host((host.as_str(), port)),
        )
        .await
        .map_err(|_| AppError::Provider("unsubscribe host lookup timed out".into()))?
        .map_err(|_| AppError::Provider("unsubscribe host did not resolve".into()))?
        .collect(),
    };
    let Some(socket) = addresses.first().copied() else {
        return Err(AppError::Provider(
            "unsubscribe host did not resolve".into(),
        ));
    };
    if !relaxed && addresses.iter().any(|addr| ip_is_blocked(addr.ip())) {
        return Err(AppError::Validation(
            "unsubscribe target is a private, loopback or link-local address".into(),
        ));
    }
    let client = reqwest::Client::builder()
        .timeout(ONE_CLICK_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        // Environment proxies would connect somewhere other than the pinned socket.
        .no_proxy()
        .resolve(&host, socket)
        .build()
        .map_err(|e| AppError::Provider(format!("unsubscribe client error: {e}")))?;
    let response = client
        .post(parsed)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body("List-Unsubscribe=One-Click")
        .send()
        .await
        .map_err(|e| {
            AppError::Provider(if e.is_timeout() {
                "unsubscribe request timed out".into()
            } else {
                "unsubscribe request failed".into()
            })
        })?;
    let status = response.status();
    if status.is_success() {
        Ok(status.as_u16())
    } else {
        Err(AppError::Provider(format!(
            "unsubscribe endpoint answered {}",
            status.as_u16()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::StatusCode, response::IntoResponse, routing::post, Router};

    #[test]
    fn parses_https_and_mailto_targets_and_one_click() {
        let raw = "Subject: x\r\nList-Unsubscribe: <mailto:u@example.com?subject=bye>,\r\n <https://example.com/u?id=1>, <http://example.com/plain>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click\r\n";
        let parsed = parse_list_unsubscribe(raw).unwrap();
        assert_eq!(parsed.https, vec!["https://example.com/u?id=1"]);
        assert_eq!(parsed.mailto, vec!["mailto:u@example.com?subject=bye"]);
        assert!(parsed.one_click);
    }

    #[test]
    fn one_click_needs_the_post_header_and_an_https_target() {
        let no_post = "List-Unsubscribe: <https://example.com/u>\r\n";
        assert!(!parse_list_unsubscribe(no_post).unwrap().one_click);
        let mailto_only = "List-Unsubscribe: <mailto:u@example.com>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click\r\n";
        assert!(!parse_list_unsubscribe(mailto_only).unwrap().one_click);
        assert!(parse_list_unsubscribe("List-Unsubscribe: <http://example.com/u>\r\n").is_none());
        assert!(parse_list_unsubscribe("Subject: x\r\n").is_none());
    }

    #[test]
    fn mailto_targets_are_validated() {
        let (to, subject, body) =
            parse_mailto("mailto:unsub@example.com?subject=Remove%20me&body=please").unwrap();
        assert_eq!(
            (to.as_str(), subject.as_str(), body.as_str()),
            ("unsub@example.com", "Remove me", "please")
        );
        assert!(parse_mailto("mailto:a@example.com,b@example.com").is_err());
        assert!(
            parse_mailto("mailto:a@example.com?subject=x%0d%0aBcc:%20evil@example.com").is_err()
        );
        assert!(parse_mailto("mailto:nobody").is_err());
    }

    #[tokio::test]
    async fn one_click_refuses_non_https_and_private_targets() {
        for url in [
            "http://example.com/u",
            "https://127.0.0.1/u",
            "https://169.254.169.254/latest",
            "https://10.0.0.5/u",
            "https://[::1]/u",
            "https://user:pw@example.com/u",
        ] {
            assert!(post_one_click(url, false).await.is_err(), "{url}");
        }
    }

    async fn serve(status: StatusCode, location: Option<&'static str>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let app = Router::new().route(
                "/",
                post(move |body: String| async move {
                    assert_eq!(body, "List-Unsubscribe=One-Click");
                    match location {
                        Some(location) => (status, [("Location", location)], "").into_response(),
                        None => (status, "").into_response(),
                    }
                }),
            );
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{address}/")
    }

    #[tokio::test]
    async fn one_click_posts_and_reports_success_status() {
        let url = serve(StatusCode::OK, None).await;
        assert_eq!(post_one_click(&url, true).await.unwrap(), 200);
    }

    #[tokio::test]
    async fn one_click_does_not_follow_redirects() {
        let url = serve(StatusCode::FOUND, Some("http://169.254.169.254/latest")).await;
        assert!(post_one_click(&url, true).await.is_err());
        let url = serve(StatusCode::INTERNAL_SERVER_ERROR, None).await;
        assert!(post_one_click(&url, true).await.is_err());
    }
}
