use std::sync::Arc;

use axum::{
    middleware,
    routing::{get, patch, post, put},
    Json, Router,
};

use crate::state::AppState;

pub mod accounts;
pub mod auth;
pub mod calendar;
pub mod compose;
pub mod events;
pub mod folders;
pub mod labels;
pub mod messages;
pub mod rules;
pub mod search;
pub mod smart_folders;
pub mod snooze;
pub mod webhooks;

pub const PRIVILEGED_ROUTE_INVENTORY: &[(&str, &str)] = &[
    ("GET", "/accounts"),
    ("POST", "/accounts"),
    ("PATCH", "/accounts/{id}"),
    ("DELETE", "/accounts/{id}"),
    ("GET", "/accounts/{id}/settings"),
    ("GET", "/accounts/{id}/folders"),
    ("PATCH", "/folders/{id}"),
    ("POST", "/folders/{id}/mark-read"),
    ("GET", "/folders/{id}/messages"),
    ("POST", "/messages/bulk"),
    ("GET", "/messages/{id}"),
    ("PATCH", "/messages/{id}"),
    ("DELETE", "/messages/{id}"),
    ("GET", "/messages/{id}/attachments"),
    ("GET", "/messages/{id}/attachments/{attachment_id}"),
    ("POST", "/messages/{id}/archive"),
    ("GET", "/messages/{id}/labels"),
    ("POST", "/messages/{id}/labels/{label_id}"),
    ("DELETE", "/messages/{id}/labels/{label_id}"),
    ("POST", "/messages/{id}/snooze"),
    ("DELETE", "/messages/{id}/snooze"),
    ("POST", "/messages"),
    ("GET", "/calendar/events"),
    ("GET", "/calendar/events/{id}"),
    ("GET", "/calendar/events/{id}/links"),
    ("POST", "/calendar/events/{id}/links"),
    ("DELETE", "/calendar/events/{id}/links/{message_id}"),
    ("POST", "/sync/{account_id}"),
    ("GET", "/auth/gmail/authorize"),
    ("GET", "/auth/microsoft/authorize"),
    ("GET", "/events"),
    ("GET", "/search"),
    ("GET", "/search/suggest"),
    ("GET", "/contacts/suggest"),
    ("GET", "/smart-folders/{kind}/messages"),
    ("GET", "/labels"),
    ("POST", "/labels"),
    ("PUT", "/labels/{id}"),
    ("DELETE", "/labels/{id}"),
    ("GET", "/labels/{id}/messages"),
    ("GET", "/rules"),
    ("POST", "/rules"),
    ("PUT", "/rules/{id}"),
    ("DELETE", "/rules/{id}"),
    ("POST", "/rules/{id}/toggle"),
    ("GET", "/app-auth/me"),
    ("POST", "/app-auth/logout"),
    ("POST", "/app-auth/refresh"),
    ("DELETE", "/app-auth/sessions/{id}"),
    ("GET", "/webhooks"),
    ("POST", "/webhooks"),
    ("PATCH", "/webhooks/{id}"),
    ("DELETE", "/webhooks/{id}"),
];

pub fn openapi_value() -> serde_json::Value {
    let mut paths = serde_json::Map::new();
    for (method, path) in PRIVILEGED_ROUTE_INVENTORY {
        paths
            .entry((*path).to_string())
            .or_insert_with(|| serde_json::json!({}));
        paths.get_mut(*path).and_then(serde_json::Value::as_object_mut).unwrap().insert(
            method.to_ascii_lowercase(),
            serde_json::json!({
                "operationId": format!("{}_{}", method.to_ascii_lowercase(), path.trim_matches('/').replace(['/', '{', '}'], "_")),
                "security": [{"bearerAuth": []}],
                "responses": {"200": {"description": "Success"}, "401": {"description": "Authentication required"}}
            }),
        );
    }
    serde_json::json!({
        "openapi": "3.1.0",
        "info": {"title": "email-rs API", "version": env!("CARGO_PKG_VERSION")},
        "servers": [{"url": "/api/v1"}],
        "paths": paths,
        "components": {"securitySchemes": {"bearerAuth": {"type": "http", "scheme": "bearer"}}}
    })
}

pub fn router(state: Arc<AppState>) -> Router {
    let protected = Router::new()
        .route(
            "/accounts",
            get(accounts::list_accounts).post(accounts::create_account),
        )
        .route(
            "/accounts/{id}",
            patch(accounts::update_account).delete(accounts::delete_account),
        )
        .route(
            "/accounts/{id}/settings",
            get(accounts::get_account_settings),
        )
        .route("/accounts/{id}/folders", get(folders::list_folders))
        .route("/folders/{id}", patch(folders::patch_folder))
        .route("/folders/{id}/mark-read", post(folders::mark_folder_read))
        .route("/folders/{id}/messages", get(messages::list_messages))
        .route("/messages/bulk", post(messages::bulk_messages))
        .route(
            "/messages/{id}",
            get(messages::get_message)
                .patch(messages::patch_message)
                .delete(messages::delete_message),
        )
        .route(
            "/messages/{id}/attachments",
            get(messages::list_attachments),
        )
        .route(
            "/messages/{id}/attachments/{attachment_id}",
            get(messages::download_attachment),
        )
        .route("/messages/{id}/archive", post(messages::archive_message))
        .route("/messages/{id}/labels", get(labels::get_message_labels))
        .route(
            "/messages/{id}/labels/{label_id}",
            post(labels::add_message_label).delete(labels::remove_message_label),
        )
        .route(
            "/messages/{id}/snooze",
            post(snooze::snooze_message).delete(snooze::unsnooze_message),
        )
        .route("/messages", post(compose::send_message))
        .route("/calendar/events", get(calendar::list_events))
        .route("/calendar/events/{id}", get(calendar::get_event))
        .route(
            "/calendar/events/{id}/links",
            get(calendar::list_event_links).post(calendar::add_event_link),
        )
        .route(
            "/calendar/events/{id}/links/{message_id}",
            axum::routing::delete(calendar::remove_event_link),
        )
        .route("/sync/{account_id}", post(accounts::trigger_sync))
        .route("/auth/gmail/authorize", get(auth::gmail_authorize))
        .route("/auth/microsoft/authorize", get(auth::microsoft_authorize))
        .route("/events", get(events::sse_events))
        .route("/search", get(search::search_messages))
        .route("/search/suggest", get(search::suggest_messages))
        .route("/contacts/suggest", get(search::suggest_contacts))
        .route(
            "/smart-folders/{kind}/messages",
            get(smart_folders::list_smart_messages),
        )
        .route(
            "/labels",
            get(labels::list_labels).post(labels::create_label),
        )
        .route(
            "/labels/{id}",
            put(labels::update_label).delete(labels::delete_label),
        )
        .route("/labels/{id}/messages", get(labels::list_label_messages))
        .route("/rules", get(rules::list_rules).post(rules::create_rule))
        .route(
            "/rules/{id}",
            put(rules::update_rule).delete(rules::delete_rule),
        )
        .route("/rules/{id}/toggle", post(rules::toggle_rule))
        .route("/app-auth/me", get(crate::auth::app::me))
        .route("/app-auth/logout", post(crate::auth::app::logout))
        .route("/app-auth/refresh", post(crate::auth::app::refresh))
        .route(
            "/app-auth/sessions/{id}",
            axum::routing::delete(crate::auth::app::revoke),
        )
        .route(
            "/webhooks",
            get(webhooks::list_webhooks).post(webhooks::create_webhook),
        )
        .route(
            "/webhooks/{id}",
            patch(webhooks::update_webhook).delete(webhooks::delete_webhook),
        )
        .route_layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            crate::auth::app::require_auth,
        ));

    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/openapi.json", get(|| async { Json(openapi_value()) }))
        .route("/app-auth/bootstrap", post(crate::auth::app::bootstrap))
        .route("/app-auth/login", post(crate::auth::app::login))
        .route("/auth/gmail/callback", get(auth::gmail_callback))
        .route("/auth/microsoft/callback", get(auth::microsoft_callback))
        .merge(protected)
        .with_state(state)
}
