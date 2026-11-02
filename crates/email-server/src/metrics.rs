//! Process-wide Prometheus metrics, rendered in the text exposition format.
//!
//! Counters are plain atomics and the HTTP table is a small mutex-guarded map,
//! so no metrics crate is needed. HTTP series are labelled with the matched
//! route template (never the raw path) to keep cardinality bounded.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use axum::{
    extract::{MatchedPath, Request},
    http::{header, HeaderValue},
    middleware::Next,
    response::{IntoResponse, Response},
};

pub static SYNC_RUNS: AtomicU64 = AtomicU64::new(0);
pub static SYNC_ERRORS: AtomicU64 = AtomicU64::new(0);
pub static MESSAGES_SYNCED: AtomicU64 = AtomicU64::new(0);
pub static WEBHOOK_DELIVERIES: AtomicU64 = AtomicU64::new(0);
pub static WEBHOOK_FAILURES: AtomicU64 = AtomicU64::new(0);

const BUCKETS: [f64; 9] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0];

#[derive(Default)]
struct HttpSeries {
    count: u64,
    sum_secs: f64,
    buckets: [u64; BUCKETS.len()],
}

type HttpKey = (String, String, u16);
static HTTP: Mutex<BTreeMap<HttpKey, HttpSeries>> = Mutex::new(BTreeMap::new());

pub fn inc(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

pub fn add(counter: &AtomicU64, n: u64) {
    counter.fetch_add(n, Ordering::Relaxed);
}

pub fn record_http(method: &str, route: &str, status: u16, secs: f64) {
    let mut map = HTTP.lock().unwrap_or_else(|e| e.into_inner());
    let series = map
        .entry((method.to_string(), route.to_string(), status))
        .or_default();
    series.count += 1;
    series.sum_secs += secs;
    for (i, bound) in BUCKETS.iter().enumerate() {
        if secs <= *bound {
            series.buckets[i] += 1;
        }
    }
}

/// Middleware recording request count and latency per matched route.
pub async fn track_http(request: Request, next: Next) -> Response {
    let method = request.method().as_str().to_string();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "unmatched".to_string());
    let started = Instant::now();
    let response = next.run(request).await;
    record_http(
        &method,
        &route,
        response.status().as_u16(),
        started.elapsed().as_secs_f64(),
    );
    response
}

fn counter(out: &mut String, name: &str, help: &str, value: &AtomicU64) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} counter");
    let _ = writeln!(out, "{name} {}", value.load(Ordering::Relaxed));
}

pub fn render() -> String {
    let mut out = String::new();
    counter(
        &mut out,
        "email_rs_sync_runs_total",
        "Account sync runs started.",
        &SYNC_RUNS,
    );
    counter(
        &mut out,
        "email_rs_sync_errors_total",
        "Account sync runs that failed.",
        &SYNC_ERRORS,
    );
    counter(
        &mut out,
        "email_rs_messages_synced_total",
        "New messages stored by sync.",
        &MESSAGES_SYNCED,
    );
    counter(
        &mut out,
        "email_rs_webhook_deliveries_total",
        "Webhook deliveries that succeeded.",
        &WEBHOOK_DELIVERIES,
    );
    counter(
        &mut out,
        "email_rs_webhook_failures_total",
        "Webhook delivery attempts that failed.",
        &WEBHOOK_FAILURES,
    );

    let map = HTTP.lock().unwrap_or_else(|e| e.into_inner());
    out.push_str("# HELP email_rs_http_requests_total HTTP requests handled.\n");
    out.push_str("# TYPE email_rs_http_requests_total counter\n");
    for ((method, route, status), s) in map.iter() {
        let _ = writeln!(
            out,
            "email_rs_http_requests_total{{method=\"{method}\",route=\"{route}\",status=\"{status}\"}} {}",
            s.count
        );
    }
    out.push_str("# HELP email_rs_http_request_duration_seconds HTTP request latency.\n");
    out.push_str("# TYPE email_rs_http_request_duration_seconds histogram\n");
    for ((method, route, status), s) in map.iter() {
        let labels = format!("method=\"{method}\",route=\"{route}\",status=\"{status}\"");
        for (i, bound) in BUCKETS.iter().enumerate() {
            let _ = writeln!(
                out,
                "email_rs_http_request_duration_seconds_bucket{{{labels},le=\"{bound}\"}} {}",
                s.buckets[i]
            );
        }
        let _ = writeln!(
            out,
            "email_rs_http_request_duration_seconds_bucket{{{labels},le=\"+Inf\"}} {}",
            s.count
        );
        let _ = writeln!(
            out,
            "email_rs_http_request_duration_seconds_sum{{{labels}}} {}",
            s.sum_secs
        );
        let _ = writeln!(
            out,
            "email_rs_http_request_duration_seconds_count{{{labels}}} {}",
            s.count
        );
    }
    out
}

/// `GET /metrics` handler (registered behind the authenticated route layer).
pub async fn handler() -> Response {
    let mut response = render().into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    response
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::auth::app::AuthSettings;
    use crate::state::AppState;

    async fn app(auth_enabled: bool) -> axum::Router {
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let path = dir.path().join("email.db").display().to_string();
        let (pool, has_fts) = crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap();
        let mut auth = AuthSettings::disabled();
        auth.enabled = auth_enabled;
        crate::api::router(Arc::new(AppState::with_auth(pool, has_fts, auth)))
    }

    async fn get(app: axum::Router, uri: &str) -> (StatusCode, Option<String>, String) {
        let response = app
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let content_type = response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap().to_string());
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            content_type,
            String::from_utf8_lossy(&body).into_owned(),
        )
    }

    #[tokio::test]
    async fn metrics_requires_authentication() {
        let (status, _, body) = get(app(true).await, "/metrics").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(!body.contains("email_rs_sync_runs_total"));
    }

    #[tokio::test]
    async fn metrics_exposes_counters_and_http_series() {
        super::inc(&super::SYNC_RUNS);
        super::add(&super::MESSAGES_SYNCED, 3);
        super::inc(&super::WEBHOOK_DELIVERIES);
        super::inc(&super::WEBHOOK_FAILURES);
        super::inc(&super::SYNC_ERRORS);
        let router = app(false).await;
        let (status, _, _) = get(router.clone(), "/health").await;
        assert_eq!(status, StatusCode::OK);

        let (status, content_type, body) = get(router, "/metrics").await;
        assert_eq!(status, StatusCode::OK);
        assert!(content_type.unwrap().starts_with("text/plain"));
        for name in [
            "email_rs_sync_runs_total",
            "email_rs_sync_errors_total",
            "email_rs_messages_synced_total",
            "email_rs_webhook_deliveries_total",
            "email_rs_webhook_failures_total",
        ] {
            assert!(body.contains(&format!("# TYPE {name} counter")), "{name}");
        }
        assert!(body.contains(
            "email_rs_http_requests_total{method=\"GET\",route=\"/health\",status=\"200\"}"
        ));
        assert!(body.contains("email_rs_http_request_duration_seconds_bucket{method=\"GET\",route=\"/health\",status=\"200\",le=\"+Inf\"}"));
        assert!(body.contains("email_rs_http_request_duration_seconds_count{method=\"GET\",route=\"/health\",status=\"200\"}"));
    }

    #[test]
    fn record_http_fills_cumulative_buckets() {
        super::record_http("GET", "/t", 200, 0.03);
        let out = super::render();
        assert!(out.contains("route=\"/t\",status=\"200\",le=\"0.025\"} 0"));
        assert!(out.contains("route=\"/t\",status=\"200\",le=\"0.05\"} 1"));
    }
}
