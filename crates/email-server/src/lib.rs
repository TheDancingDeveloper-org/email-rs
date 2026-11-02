pub mod api;
pub mod attachment_store;
pub mod auth;
pub mod calendar;
pub mod clean;
pub mod config;
pub mod credentials;
pub mod db;
pub mod error;
pub mod imap;
pub mod maintenance;
pub mod metrics;
pub mod outbox;
pub mod providers;
pub mod rules;
pub mod sanitize;
pub mod search_index;
pub mod smtp;
pub mod state;
pub mod sync;
pub mod threading;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{http::HeaderName, routing::get_service, Router};
pub use config::Config;
use state::AppState;
use sync::SyncOrchestrator;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};
use tracing::info;

pub async fn run_server(cfg: Config) -> anyhow::Result<()> {
    cfg.validate_security()?;
    info!("starting email-server on {}", cfg.bind_addr());

    let (pool, has_fts) = db::create_pool(&cfg.database_url).await?;
    credentials::migrate_account_credentials(&pool).await?;
    auth::app::bootstrap_from_env(&pool).await?;
    let auth_settings = auth::app::AuthSettings::from_env(cfg.public_url.as_deref());
    let app_state = Arc::new(AppState::with_auth(pool.clone(), has_fts, auth_settings));
    api::webhooks::spawn_delivery_worker(pool.clone(), cfg.webhook_policy.clone());
    clean::spawn_backfill(pool.clone());
    if has_fts {
        search_index::spawn_reindex(pool.clone());
    }
    maintenance::spawn(pool.clone(), has_fts, cfg.maintenance.clone());
    SyncOrchestrator::spawn_background(pool, app_state.event_tx.clone());
    outbox::spawn_worker(app_state.pool.clone());

    let app = build_app(app_state, &cfg.frontend_dist);

    let addr: SocketAddr = cfg.bind_addr().parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("listening on {}", addr);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

pub fn build_app(app_state: Arc<AppState>, frontend_dist: &str) -> Router {
    let allowed_origins = Arc::clone(&app_state.auth.allowed_origins);
    let api_routes = api::router(app_state);
    let static_files = ServeDir::new(frontend_dist).append_index_html_on_directories(true);

    Router::new()
        .nest("/api/v1", api_routes)
        .fallback_service(get_service(static_files))
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::predicate(move |origin, _| {
                    origin
                        .to_str()
                        .ok()
                        .is_some_and(|origin| allowed_origins.contains(origin))
                }))
                .allow_credentials(true)
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::PUT,
                    axum::http::Method::PATCH,
                    axum::http::Method::DELETE,
                    axum::http::Method::OPTIONS,
                ])
                .allow_headers([
                    axum::http::header::AUTHORIZATION,
                    axum::http::header::CONTENT_TYPE,
                    HeaderName::from_static("x-requested-with"),
                ]),
        )
        .layer(axum::middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
}

/// Adds baseline security headers to every response. Handlers that set their
/// own value for one of these (e.g. attachment downloads) keep it.
async fn security_headers(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    for (name, value) in [
        ("x-content-type-options", "nosniff"),
        (
            "content-security-policy",
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; object-src 'none'; base-uri 'self'; frame-ancestors 'none'",
        ),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "no-referrer"),
    ] {
        headers
            .entry(HeaderName::from_static(name))
            .or_insert(axum::http::HeaderValue::from_static(value));
    }
    res
}
