//! Schema-bearing API contract. Documentation functions are deliberately kept
//! separate from handlers so the wire contract can be reviewed and generated
//! without coupling Axum routing to a documentation framework.

#![allow(dead_code)]

use serde::Serialize;
use utoipa::{Modify, OpenApi, ToSchema};

use email_types::{
    Account, AccountSettings, AddLinkBody, AddLinkResponse, AuthCredentials, AuthIdentity,
    AuthSession, AuthorizeResponse, BulkRequest, BulkResponse, CalendarEvent, Contact,
    CreateAccount, CreateLabelBody, CreateRuleBody, CreateWebhook, DraftRequest, DraftResponse,
    EventLink, Folder, FullMessage, Label, Message, MessageAttachment, PatchFolder,
    PatchMessageRequest, RuleDto, SearchResponse, SendRequest, SendResponse, SnoozeRequest,
    StatusResponse, SuggestResponse, UpdateAccount, UpdateLabelBody, UpdateRuleBody, UpdateWebhook,
    Webhook,
};

use super::integration_tokens::{
    CreateIntegrationToken, CreatedIntegrationToken, IntegrationToken,
};
use super::webhooks::{IntegrationEvent, WebhookDelivery};

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VersionResponse {
    pub product_version: String,
    pub api_version: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitiesResponse {
    pub product_version: String,
    pub api_version: String,
    pub capabilities: Vec<String>,
    pub integration_scopes: Vec<String>,
}

#[utoipa::path(get, path = "/health", responses((status = 200, body = String)))]
fn doc_health() {}

#[utoipa::path(get, path = "/openapi.json", responses((status = 200, description = "OpenAPI 3.1 contract")))]
fn doc_openapi() {}

#[utoipa::path(
    get,
    path = "/ready",
    responses(
        (status = 200, body = StatusResponse),
        (status = 503, body = ErrorResponse)
    )
)]
fn doc_ready() {}

#[utoipa::path(get, path = "/version", responses((status = 200, body = VersionResponse)))]
fn doc_version() {}

#[utoipa::path(get, path = "/capabilities", responses((status = 200, body = CapabilitiesResponse)))]
fn doc_capabilities() {}

#[utoipa::path(
    post,
    path = "/app-auth/bootstrap",
    request_body = AuthCredentials,
    responses(
        (status = 200, body = AuthSession),
        (status = 400, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 409, body = ErrorResponse)
    )
)]
fn doc_bootstrap() {}

#[utoipa::path(
    post,
    path = "/app-auth/login",
    request_body = AuthCredentials,
    responses(
        (status = 200, body = AuthSession),
        (status = 401, body = ErrorResponse)
    )
)]
fn doc_login() {}

#[utoipa::path(
    get,
    path = "/integration-tokens",
    security(("bearerAuth" = [])),
    responses((status = 200, body = [IntegrationToken]), (status = 401, body = ErrorResponse))
)]
fn doc_list_integration_tokens() {}

#[utoipa::path(
    post,
    path = "/integration-tokens",
    security(("bearerAuth" = [])),
    request_body = CreateIntegrationToken,
    responses(
        (status = 201, body = CreatedIntegrationToken),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse)
    )
)]
fn doc_create_integration_token() {}

#[utoipa::path(
    delete,
    path = "/integration-tokens/{id}",
    security(("bearerAuth" = [])),
    params(("id" = String, Path, description = "Integration token ID")),
    responses(
        (status = 200, body = StatusResponse),
        (status = 404, body = ErrorResponse)
    )
)]
fn doc_revoke_integration_token() {}

#[utoipa::path(
    get,
    path = "/accounts",
    security(("bearerAuth" = ["mail.accounts.read"])),
    responses((status = 200, body = [Account]), (status = 401, body = ErrorResponse), (status = 403, body = ErrorResponse))
)]
fn doc_list_accounts() {}

#[utoipa::path(
    post,
    path = "/accounts",
    security(("bearerAuth" = [])),
    request_body = CreateAccount,
    responses((status = 200, body = Account), (status = 401, body = ErrorResponse))
)]
fn doc_create_account() {}

#[utoipa::path(
    patch,
    path = "/accounts/{id}",
    security(("bearerAuth" = [])),
    params(("id" = String, Path)),
    request_body = UpdateAccount,
    responses((status = 200, body = Account), (status = 404, body = ErrorResponse))
)]
fn doc_update_account() {}

#[utoipa::path(
    delete,
    path = "/accounts/{id}",
    security(("bearerAuth" = [])),
    params(("id" = String, Path)),
    responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse))
)]
fn doc_delete_account() {}

#[utoipa::path(
    get,
    path = "/accounts/{id}/settings",
    security(("bearerAuth" = ["mail.accounts.read"])),
    params(("id" = String, Path)),
    responses((status = 200, body = AccountSettings), (status = 404, body = ErrorResponse))
)]
fn doc_account_settings() {}

#[utoipa::path(
    get,
    path = "/accounts/{id}/folders",
    security(("bearerAuth" = ["mail.accounts.read"])),
    params(("id" = String, Path)),
    responses((status = 200, body = [Folder]))
)]
fn doc_list_folders() {}

#[utoipa::path(
    patch,
    path = "/folders/{id}",
    security(("bearerAuth" = ["mail.modify"])),
    params(("id" = String, Path)),
    request_body = PatchFolder,
    responses((status = 200, body = Folder), (status = 404, body = ErrorResponse))
)]
fn doc_patch_folder() {}

#[utoipa::path(
    post,
    path = "/folders/{id}/mark-read",
    security(("bearerAuth" = ["mail.modify"])),
    params(("id" = String, Path)),
    responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse))
)]
fn doc_mark_folder_read() {}

#[utoipa::path(
    get,
    path = "/folders/{id}/messages",
    security(("bearerAuth" = ["mail.metadata.read"])),
    params(
        ("id" = String, Path),
        ("page" = Option<i64>, Query),
        ("per_page" = Option<i64>, Query),
        ("unread_only" = Option<bool>, Query)
    ),
    responses((status = 200, body = [Message]))
)]
fn doc_list_messages() {}

#[utoipa::path(
    get,
    path = "/messages/{id}",
    security(("bearerAuth" = ["mail.body.read"])),
    params(("id" = String, Path, description = "Mailbox occurrence ID")),
    responses((status = 200, body = FullMessage), (status = 404, body = ErrorResponse))
)]
fn doc_get_message() {}

#[utoipa::path(
    patch,
    path = "/messages/{id}",
    security(("bearerAuth" = ["mail.modify"])),
    params(("id" = String, Path)),
    request_body = PatchMessageRequest,
    responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse))
)]
fn doc_patch_message() {}

#[utoipa::path(
    delete,
    path = "/messages/{id}",
    security(("bearerAuth" = ["mail.modify"])),
    params(("id" = String, Path)),
    responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse))
)]
fn doc_delete_message() {}

#[utoipa::path(
    post,
    path = "/messages/{id}/archive",
    security(("bearerAuth" = ["mail.modify"])),
    params(("id" = String, Path)),
    responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse))
)]
fn doc_archive_message() {}

#[utoipa::path(
    post,
    path = "/messages/bulk",
    security(("bearerAuth" = ["mail.modify"])),
    request_body = BulkRequest,
    responses((status = 200, body = BulkResponse))
)]
fn doc_bulk_messages() {}

#[utoipa::path(
    get,
    path = "/messages/{id}/attachments",
    security(("bearerAuth" = ["mail.body.read"])),
    params(("id" = String, Path)),
    responses((status = 200, body = [MessageAttachment]))
)]
fn doc_list_attachments() {}

#[utoipa::path(
    get,
    path = "/messages/{id}/attachments/{attachment_id}",
    security(("bearerAuth" = ["mail.body.read"])),
    params(("id" = String, Path), ("attachment_id" = String, Path)),
    responses((status = 200, content_type = "application/octet-stream", body = Vec<u8>), (status = 404, body = ErrorResponse))
)]
fn doc_download_attachment() {}

#[utoipa::path(
    post,
    path = "/messages",
    security(("bearerAuth" = ["mail.send"])),
    request_body = SendRequest,
    responses((status = 200, body = SendResponse), (status = 502, body = ErrorResponse))
)]
fn doc_send_message() {}

#[utoipa::path(
    post,
    path = "/drafts",
    security(("bearerAuth" = ["mail.modify"])),
    request_body = DraftRequest,
    responses((status = 200, body = DraftResponse), (status = 404, body = ErrorResponse), (status = 502, body = ErrorResponse))
)]
fn doc_create_draft() {}

#[utoipa::path(
    put,
    path = "/drafts/{id}",
    security(("bearerAuth" = ["mail.modify"])),
    params(("id" = String, Path, description = "Draft occurrence ID")),
    request_body = DraftRequest,
    responses((status = 200, body = DraftResponse), (status = 404, body = ErrorResponse), (status = 502, body = ErrorResponse))
)]
fn doc_update_draft() {}

#[utoipa::path(
    delete,
    path = "/drafts/{id}",
    security(("bearerAuth" = ["mail.modify"])),
    params(("id" = String, Path, description = "Draft occurrence ID")),
    responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse))
)]
fn doc_delete_draft() {}

#[utoipa::path(
    post,
    path = "/sync/{account_id}",
    security(("bearerAuth" = ["mail.sync"])),
    params(("account_id" = String, Path)),
    responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse))
)]
fn doc_trigger_sync() {}

#[utoipa::path(
    get,
    path = "/auth/gmail/authorize",
    params(("account_id" = Option<String>, Query, description = "Reconnect this existing account instead of adding one")),
    security(("bearerAuth" = []), ("bearerAuth" = ["mail.accounts.connect"])),
    responses((status = 200, body = AuthorizeResponse), (status = 401, body = ErrorResponse))
)]
fn doc_gmail_authorize() {}

#[utoipa::path(get, path = "/app-auth/me", security(("bearerAuth" = [])), responses((status = 200, body = AuthIdentity), (status = 401, body = ErrorResponse)))]
fn doc_auth_me() {}

#[utoipa::path(post, path = "/app-auth/logout", security(("bearerAuth" = [])), responses((status = 200, body = StatusResponse), (status = 401, body = ErrorResponse)))]
fn doc_auth_logout() {}

#[utoipa::path(post, path = "/app-auth/refresh", security(("bearerAuth" = [])), responses((status = 200, body = AuthSession), (status = 401, body = ErrorResponse)))]
fn doc_auth_refresh() {}

#[utoipa::path(delete, path = "/app-auth/sessions/{id}", security(("bearerAuth" = [])), params(("id" = String, Path)), responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse)))]
fn doc_revoke_session() {}

#[utoipa::path(get, path = "/auth/microsoft/authorize", params(("account_id" = Option<String>, Query, description = "Reconnect this existing account instead of adding one")), security(("bearerAuth" = []), ("bearerAuth" = ["mail.accounts.connect"])), responses((status = 200, body = AuthorizeResponse), (status = 401, body = ErrorResponse)))]
fn doc_microsoft_authorize() {}

#[utoipa::path(get, path = "/auth/gmail/callback", params(("code" = String, Query), ("state" = String, Query)), responses((status = 303, description = "OAuth flow completed"), (status = 400, body = ErrorResponse)))]
fn doc_gmail_callback() {}

#[utoipa::path(get, path = "/auth/microsoft/callback", params(("code" = String, Query), ("state" = String, Query)), responses((status = 303, description = "OAuth flow completed"), (status = 400, body = ErrorResponse)))]
fn doc_microsoft_callback() {}

#[utoipa::path(get, path = "/smart-folders/{kind}/messages", security(("bearerAuth" = ["mail.metadata.read"])), params(("kind" = String, Path), ("page" = Option<i64>, Query), ("per_page" = Option<i64>, Query)), responses((status = 200, body = [Message]), (status = 404, body = ErrorResponse)))]
fn doc_smart_messages() {}

#[utoipa::path(get, path = "/search", security(("bearerAuth" = ["mail.metadata.read"])), params(("q" = Option<String>, Query), ("account_id" = Option<String>, Query), ("match" = Option<String>, Query), ("from" = Option<String>, Query), ("to" = Option<String>, Query), ("cc" = Option<String>, Query), ("subject" = Option<String>, Query), ("body" = Option<String>, Query), ("filename" = Option<String>, Query), ("has_attachment" = Option<bool>, Query), ("is_read" = Option<bool>, Query), ("is_flagged" = Option<bool>, Query), ("date_after" = Option<String>, Query), ("date_before" = Option<String>, Query)), responses((status = 200, body = SearchResponse)))]
fn doc_search() {}

#[utoipa::path(get, path = "/search/suggest", security(("bearerAuth" = ["mail.metadata.read"])), params(("q" = String, Query), ("account_id" = Option<String>, Query)), responses((status = 200, body = SuggestResponse)))]
fn doc_search_suggest() {}

#[utoipa::path(get, path = "/contacts/suggest", security(("bearerAuth" = ["mail.metadata.read"])), params(("q" = String, Query)), responses((status = 200, body = [Contact])))]
fn doc_contacts_suggest() {}

#[utoipa::path(post, path = "/messages/{id}/snooze", security(("bearerAuth" = ["mail.modify"])), params(("id" = String, Path)), request_body = SnoozeRequest, responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse)))]
fn doc_snooze_message() {}

#[utoipa::path(delete, path = "/messages/{id}/snooze", security(("bearerAuth" = ["mail.modify"])), params(("id" = String, Path)), responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse)))]
fn doc_unsnooze_message() {}

#[utoipa::path(get, path = "/labels", security(("bearerAuth" = ["mail.metadata.read"])), params(("account_id" = String, Query)), responses((status = 200, body = [Label])))]
fn doc_list_labels() {}

#[utoipa::path(post, path = "/labels", security(("bearerAuth" = [])), request_body = CreateLabelBody, responses((status = 200, body = Label)))]
fn doc_create_label() {}

#[utoipa::path(put, path = "/labels/{id}", security(("bearerAuth" = [])), params(("id" = String, Path)), request_body = UpdateLabelBody, responses((status = 200, body = Label), (status = 404, body = ErrorResponse)))]
fn doc_update_label() {}

#[utoipa::path(delete, path = "/labels/{id}", security(("bearerAuth" = [])), params(("id" = String, Path)), responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse)))]
fn doc_delete_label() {}

#[utoipa::path(get, path = "/labels/{id}/messages", security(("bearerAuth" = ["mail.metadata.read"])), params(("id" = String, Path), ("page" = Option<i64>, Query), ("per_page" = Option<i64>, Query)), responses((status = 200, body = [Message])))]
fn doc_label_messages() {}

#[utoipa::path(get, path = "/messages/{id}/labels", security(("bearerAuth" = ["mail.metadata.read"])), params(("id" = String, Path)), responses((status = 200, body = [Label])))]
fn doc_message_labels() {}

#[utoipa::path(post, path = "/messages/{id}/labels/{label_id}", security(("bearerAuth" = ["mail.modify"])), params(("id" = String, Path), ("label_id" = String, Path)), responses((status = 200, body = StatusResponse)))]
fn doc_add_message_label() {}

#[utoipa::path(delete, path = "/messages/{id}/labels/{label_id}", security(("bearerAuth" = ["mail.modify"])), params(("id" = String, Path), ("label_id" = String, Path)), responses((status = 200, body = StatusResponse)))]
fn doc_remove_message_label() {}

#[utoipa::path(get, path = "/calendar/events", security(("bearerAuth" = [])), params(("account_id" = Option<String>, Query), ("from" = Option<String>, Query), ("to" = Option<String>, Query)), responses((status = 200, body = [CalendarEvent])))]
fn doc_calendar_events() {}

#[utoipa::path(get, path = "/calendar/events/{id}", security(("bearerAuth" = [])), params(("id" = String, Path)), responses((status = 200, body = CalendarEvent), (status = 404, body = ErrorResponse)))]
fn doc_calendar_event() {}

#[utoipa::path(get, path = "/calendar/events/{id}/links", security(("bearerAuth" = [])), params(("id" = String, Path)), responses((status = 200, body = [EventLink])))]
fn doc_event_links() {}

#[utoipa::path(post, path = "/calendar/events/{id}/links", security(("bearerAuth" = [])), params(("id" = String, Path)), request_body = AddLinkBody, responses((status = 200, body = AddLinkResponse)))]
fn doc_add_event_link() {}

#[utoipa::path(delete, path = "/calendar/events/{id}/links/{message_id}", security(("bearerAuth" = [])), params(("id" = String, Path), ("message_id" = String, Path)), responses((status = 204, description = "Link removed")))]
fn doc_remove_event_link() {}

#[utoipa::path(get, path = "/rules", security(("bearerAuth" = [])), params(("account_id" = String, Query)), responses((status = 200, body = [RuleDto])))]
fn doc_list_rules() {}

#[utoipa::path(post, path = "/rules", security(("bearerAuth" = [])), request_body = CreateRuleBody, responses((status = 200, body = RuleDto)))]
fn doc_create_rule() {}

#[utoipa::path(put, path = "/rules/{id}", security(("bearerAuth" = [])), params(("id" = String, Path)), request_body = UpdateRuleBody, responses((status = 200, body = RuleDto), (status = 404, body = ErrorResponse)))]
fn doc_update_rule() {}

#[utoipa::path(delete, path = "/rules/{id}", security(("bearerAuth" = [])), params(("id" = String, Path)), responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse)))]
fn doc_delete_rule() {}

#[utoipa::path(post, path = "/rules/{id}/toggle", security(("bearerAuth" = [])), params(("id" = String, Path)), responses((status = 200, body = RuleDto), (status = 404, body = ErrorResponse)))]
fn doc_toggle_rule() {}

#[utoipa::path(get, path = "/webhooks", security(("bearerAuth" = [])), responses((status = 200, body = [Webhook])))]
fn doc_list_webhooks() {}

#[utoipa::path(post, path = "/webhooks", security(("bearerAuth" = [])), request_body = CreateWebhook, responses((status = 200, body = Webhook)))]
fn doc_create_webhook() {}

#[utoipa::path(patch, path = "/webhooks/{id}", security(("bearerAuth" = [])), params(("id" = String, Path)), request_body = UpdateWebhook, responses((status = 200, body = Webhook), (status = 404, body = ErrorResponse)))]
fn doc_update_webhook() {}

#[utoipa::path(delete, path = "/webhooks/{id}", security(("bearerAuth" = [])), params(("id" = String, Path)), responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse)))]
fn doc_delete_webhook() {}

#[utoipa::path(get, path = "/integration-events", security(("bearerAuth" = ["mail.metadata.read"])), params(("after" = Option<String>, Query, description = "Return events after this event ID"), ("limit" = Option<i64>, Query)), responses((status = 200, body = [IntegrationEvent])))]
fn doc_integration_events() {}

#[utoipa::path(get, path = "/webhook-deliveries", security(("bearerAuth" = [])), responses((status = 200, body = [WebhookDelivery])))]
fn doc_webhook_deliveries() {}

#[utoipa::path(get, path = "/events", security(("bearerAuth" = ["mail.metadata.read"])), responses((status = 200, content_type = "text/event-stream", body = String)))]
fn doc_events() {}

#[derive(OpenApi)]
#[openapi(
    info(title = "email-rs API", version = "0.6.3"),
    servers((url = "/api/v1")),
    paths(
        doc_health,
        doc_openapi,
        doc_ready,
        doc_version,
        doc_capabilities,
        doc_bootstrap,
        doc_login,
        doc_list_integration_tokens,
        doc_create_integration_token,
        doc_revoke_integration_token,
        doc_list_accounts,
        doc_create_account,
        doc_update_account,
        doc_delete_account,
        doc_account_settings,
        doc_list_folders,
        doc_patch_folder,
        doc_mark_folder_read,
        doc_list_messages,
        doc_get_message,
        doc_patch_message,
        doc_delete_message,
        doc_archive_message,
        doc_bulk_messages,
        doc_list_attachments,
        doc_download_attachment,
        doc_send_message,
        doc_create_draft,
        doc_update_draft,
        doc_delete_draft,
        doc_trigger_sync,
        doc_gmail_authorize,
        doc_auth_me,
        doc_auth_logout,
        doc_auth_refresh,
        doc_revoke_session,
        doc_microsoft_authorize,
        doc_gmail_callback,
        doc_microsoft_callback,
        doc_smart_messages,
        doc_search,
        doc_search_suggest,
        doc_contacts_suggest,
        doc_snooze_message,
        doc_unsnooze_message,
        doc_list_labels,
        doc_create_label,
        doc_update_label,
        doc_delete_label,
        doc_label_messages,
        doc_message_labels,
        doc_add_message_label,
        doc_remove_message_label,
        doc_calendar_events,
        doc_calendar_event,
        doc_event_links,
        doc_add_event_link,
        doc_remove_event_link,
        doc_list_rules,
        doc_create_rule,
        doc_update_rule,
        doc_delete_rule,
        doc_toggle_rule,
        doc_list_webhooks,
        doc_create_webhook,
        doc_update_webhook,
        doc_delete_webhook,
        doc_integration_events,
        doc_webhook_deliveries,
        doc_events
    ),
    components(schemas(
        ErrorResponse,
        VersionResponse,
        CapabilitiesResponse,
        StatusResponse,
        AuthCredentials,
        AuthSession,
        AuthIdentity,
        IntegrationToken,
        CreateIntegrationToken,
        CreatedIntegrationToken,
        Account,
        CreateAccount,
        UpdateAccount,
        AccountSettings,
        Folder,
        PatchFolder,
        Message,
        FullMessage,
        MessageAttachment,
        PatchMessageRequest,
        BulkRequest,
        BulkResponse,
        SendRequest,
        SendResponse,
        DraftRequest,
        DraftResponse,
        AuthorizeResponse
        , SearchResponse
        , SuggestResponse
        , Contact
        , SnoozeRequest
        , Label
        , CreateLabelBody
        , UpdateLabelBody
        , CalendarEvent
        , EventLink
        , AddLinkBody
        , AddLinkResponse
        , RuleDto
        , CreateRuleBody
        , UpdateRuleBody
        , Webhook
        , CreateWebhook
        , UpdateWebhook
        , IntegrationEvent
        , WebhookDelivery
    )),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};

        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearerAuth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("email-rs session or scoped integration token")
                        .build(),
                ),
            );
        }
    }
}

pub fn value() -> serde_json::Value {
    let mut value =
        serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI document must serialize");
    let Some(paths) = value
        .get_mut("paths")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return value;
    };
    for path in paths
        .values_mut()
        .filter_map(serde_json::Value::as_object_mut)
    {
        for operation in path
            .values_mut()
            .filter_map(serde_json::Value::as_object_mut)
        {
            if let Some(operation_id) = operation
                .get("operationId")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
            {
                operation.insert(
                    "operationId".to_string(),
                    serde_json::Value::String(
                        operation_id
                            .strip_prefix("doc_")
                            .unwrap_or(&operation_id)
                            .to_string(),
                    ),
                );
            }
            if operation.get("security").is_some() {
                let responses = operation
                    .entry("responses")
                    .or_insert_with(|| serde_json::json!({}))
                    .as_object_mut()
                    .expect("operation responses must be an object");
                for (status, description) in [
                    ("401", "Authentication required"),
                    ("403", "Credential lacks access or required scope"),
                ] {
                    responses.entry(status.to_string()).or_insert_with(|| {
                        serde_json::json!({
                            "description": description,
                            "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ErrorResponse"}}}
                        })
                    });
                }
            }
        }
    }
    value
}
