pub mod api;
pub mod auth;
pub mod calendar;
pub mod config;
pub mod db;
pub mod error;
pub mod imap;
pub mod providers;
pub mod rules;
pub mod smtp;
pub mod state;
pub mod sync;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{routing::get_service, Router};
pub use config::Config;
use state::AppState;
use sync::SyncOrchestrator;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing::info;

pub async fn run_server(cfg: Config) -> anyhow::Result<()> {
    info!("starting email-server on {}", cfg.bind_addr());

    let (pool, has_fts) = db::create_pool(&cfg.database_url).await?;
    let app_state = Arc::new(AppState::new(pool.clone(), has_fts));
    SyncOrchestrator::spawn_background(pool, app_state.event_tx.clone());

    let app = build_app(app_state, &cfg.frontend_dist);

    let addr: SocketAddr = cfg.bind_addr().parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn build_app(app_state: Arc<AppState>, frontend_dist: &str) -> Router {
    let api_routes = api::router(app_state);
    let static_files = ServeDir::new(frontend_dist).append_index_html_on_directories(true);

    Router::new()
        .nest("/api/v1", api_routes)
        .fallback_service(get_service(static_files))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
