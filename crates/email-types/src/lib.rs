use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthClientKind {
    Cookie,
    Bearer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthCredentials {
    pub username: String,
    pub password: String,
    pub client: AuthClientKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthSession {
    pub user_id: String,
    pub session_id: String,
    pub expires_at: i64,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthIdentity {
    pub user_id: String,
    pub session_id: String,
    pub client: AuthClientKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct StatusResponse {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub email: String,
    pub provider_type: String,
    pub auth_type: String,
    pub created_at: String,
    /// The provider refused the stored OAuth grant (`invalid_grant`): sync is
    /// paused until the user reconnects the account.
    #[serde(default)]
    #[cfg_attr(feature = "server", sqlx(default))]
    pub needs_reauth: bool,
    /// The provider's reason, when `needs_reauth` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "server", sqlx(default))]
    pub reauth_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateAccount {
    pub name: String,
    pub email: String,
    pub provider_type: String,
    pub auth_type: String,
    pub oauth_token_json: Option<String>,
    pub host: Option<String>,
    pub port: Option<i64>,
    pub use_ssl: Option<bool>,
    pub password: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<i64>,
    pub smtp_password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAccount {
    pub name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub sync_days_limit: Option<Option<i64>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub signature: Option<Option<String>>,
}

mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S, T>(value: &Option<Option<T>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        match value {
            Some(value) => value.serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        Option::<T>::deserialize(deserializer).map(Some)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct AccountSettings {
    pub id: String,
    pub name: String,
    pub email: String,
    pub provider_type: String,
    pub sync_days_limit: Option<i64>,
    pub signature: Option<String>,
}

/// Id of the identity every account has: its own address and display name.
pub const DEFAULT_IDENTITY_ID: &str = "default";

/// An address a message can be sent From. Each account has a default identity
/// plus any aliases added to it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub email: String,
    pub is_default: bool,
    pub signature: Option<String>,
    pub signature_html: Option<String>,
    pub signature_placement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateIdentity {
    pub name: String,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_html: Option<String>,
    #[serde(
        default = "default_signature_placement",
        skip_serializing_if = "is_below"
    )]
    pub signature_placement: String,
}

fn default_signature_placement() -> String {
    "below".to_string()
}

fn is_below(value: &str) -> bool {
    value == "below"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateIdentity {
    pub name: Option<String>,
    pub email: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub signature: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub signature_html: Option<Option<String>>,
    pub signature_placement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub full_path: String,
    pub special_use: Option<String>,
    pub delimiter: Option<String>,
    pub unread_count: i64,
    pub total_count: i64,
    pub synced_at: Option<String>,
    pub is_excluded: bool,
    #[serde(default = "default_true")]
    pub is_subscribed: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateFolder {
    /// Leaf name of the new folder; it may not contain the hierarchy separator.
    pub name: String,
    /// Folder to nest the new one under; top level when absent.
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameFolder {
    /// New leaf name; the folder stays under the same parent.
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PatchFolder {
    pub is_excluded: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// Backward-compatible mailbox-occurrence identifier.
    pub id: String,
    /// Identifies this concrete folder/UID occurrence.
    pub occurrence_id: String,
    /// Durable cross-folder identifier used by integrations.
    pub logical_id: String,
    pub account_id: String,
    pub folder_id: String,
    pub uid: i64,
    pub message_id: Option<String>,
    pub thread_id: Option<String>,
    pub subject: Option<String>,
    pub from_name: Option<String>,
    pub from_email: Option<String>,
    pub to_json: Option<String>,
    pub date: Option<String>,
    pub is_read: bool,
    pub is_flagged: bool,
    pub is_draft: bool,
    pub has_attachments: bool,
    pub preview: Option<String>,
    #[cfg_attr(feature = "server", sqlx(default))]
    pub snoozed_until: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MessageBody {
    pub html_body: Option<String>,
    pub text_body: Option<String>,
    /// Original RFC headers retained for source/export views.
    #[serde(default)]
    pub raw_headers: Option<String>,
    /// The current message as Markdown for models and search: the real text
    /// part (or the HTML converted), without quoted history, signature,
    /// images or tracking links. Computed once when the body is cached.
    #[serde(default)]
    pub clean_body: Option<String>,
    /// The quoted history split off `cleanBody`, if the message had any.
    #[serde(default)]
    pub quoted_history: Option<String>,
    /// Whether a signature was removed from `cleanBody`.
    #[serde(default)]
    pub signature_stripped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct MessageAttachment {
    pub id: String,
    pub message_id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    #[cfg_attr(feature = "server", sqlx(default))]
    #[serde(default)]
    pub dangerous_or_double_extension: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticationResultStatus {
    Pass,
    Fail,
    #[default]
    None,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MessageAuthenticationResults {
    pub spf: AuthenticationResultStatus,
    pub dkim: AuthenticationResultStatus,
    pub dmarc: AuthenticationResultStatus,
}

/// Unsubscribe targets advertised by a message's `List-Unsubscribe` header
/// (RFC 2369), with the RFC 8058 one-click flag from `List-Unsubscribe-Post`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListUnsubscribe {
    /// `https:` targets. Plain `http:` targets are never offered.
    pub https: Vec<String>,
    pub mailto: Vec<String>,
    /// True when `List-Unsubscribe-Post: List-Unsubscribe=One-Click` is present
    /// and at least one HTTPS target exists.
    pub one_click: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FullMessage {
    /// Backward-compatible mailbox-occurrence identifier.
    pub id: String,
    pub occurrence_id: String,
    pub logical_id: String,
    pub account_id: String,
    pub folder_id: String,
    pub uid: i64,
    pub message_id: Option<String>,
    pub thread_id: Option<String>,
    pub subject: Option<String>,
    pub from_name: Option<String>,
    pub from_email: Option<String>,
    pub to: Option<String>,
    pub date: Option<String>,
    pub is_read: bool,
    pub is_flagged: bool,
    pub is_draft: bool,
    pub has_attachments: bool,
    pub preview: Option<String>,
    pub body: Option<MessageBody>,
    #[serde(default)]
    pub authentication_results: MessageAuthenticationResults,
    #[serde(default)]
    pub from_reply_to_domain_mismatch: bool,
    #[serde(default)]
    pub display_name_spoofing: bool,
    #[serde(default)]
    pub list_unsubscribe: Option<ListUnsubscribe>,
    #[serde(default)]
    pub attachments: Vec<MessageAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BulkAction {
    Archive,
    Delete,
    MarkRead,
    MarkUnread,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveMessageRequest {
    pub folder_id: String,
}

/// Result of moving a message. `message_id` is the message's id in the
/// destination folder, which the move to put it back needs; it is absent when
/// the mail server did not say where the message went.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveMessageResponse {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct BulkRequest {
    pub ids: Vec<String>,
    pub action: BulkAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct BulkResult {
    pub id: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct BulkResponse {
    pub processed: usize,
    pub failed: usize,
    pub results: Vec<BulkResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PatchMessageRequest {
    pub is_read: Option<bool>,
    pub is_flagged: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct SnoozeRequest {
    pub until: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendRequest {
    pub account_id: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub text_body: Option<String>,
    pub html_body: Option<String>,
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub attachments: Vec<OutgoingAttachment>,
    #[serde(default)]
    pub send_at: Option<String>,
    #[serde(default)]
    pub send_delay_seconds: Option<u64>,
    /// Send From this identity (`GET /accounts/{id}/identities`). Omitted or
    /// `"default"` sends From the account's own address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingAttachment {
    pub filename: String,
    pub content_type: String,
    /// Standard base64-encoded file content.
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendResponse {
    pub status: String,
    /// Outbox id; pass it to `DELETE /outbox/{id}` to cancel before `cancel_until`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_until: Option<String>,
}

/// A queued send, as listed by `GET /outbox`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutboxItem {
    pub id: String,
    /// `pending`, `sending`, `sent` or `cancelled`.
    pub status: String,
    pub scheduled_at: String,
    pub cancel_until: String,
    pub attempts: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct CancelOutboxResponse {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DraftRequest {
    pub account_id: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub text_body: Option<String>,
    pub html_body: Option<String>,
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub attachments: Vec<OutgoingAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DraftResponse {
    pub id: String,
    pub occurrence_id: String,
    pub logical_id: String,
    pub message_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct AuthorizeResponse {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateLabelBody {
    pub account_id: String,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLabelBody {
    pub name: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Attendee {
    pub email: String,
    pub name: Option<String>,
    pub response_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: String,
    pub calendar_id: String,
    pub provider_event_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub location: Option<String>,
    pub is_all_day: bool,
    pub recurrence_rule: Option<String>,
    pub attendees: Vec<Attendee>,
    pub meet_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct CalendarSearchResult {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub start_at: String,
    pub end_at: String,
    pub location: Option<String>,
    pub is_all_day: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct CalendarSuggestion {
    pub id: String,
    pub title: String,
    pub start_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct MessageSuggestion {
    pub id: String,
    pub folder_id: String,
    pub subject: Option<String>,
    pub from_name: Option<String>,
    pub from_email: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct SuggestResponse {
    pub messages: Vec<MessageSuggestion>,
    pub events: Vec<CalendarSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct SearchResponse {
    pub messages: Vec<Message>,
    pub events: Vec<CalendarSearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConditionField {
    From,
    To,
    Cc,
    Subject,
    Body,
    Filename,
    HasAttachment,
    IsRead,
    IsFlagged,
    DateAfter,
    DateBefore,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOperator {
    Contains,
    NotContains,
    Equals,
    StartsWith,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct Condition {
    pub id: String,
    pub field: ConditionField,
    pub operator: ConditionOperator,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MatchMode {
    All,
    Any,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct ConditionGroup {
    #[serde(rename = "match")]
    pub match_mode: MatchMode,
    pub conditions: Vec<Condition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuleConditionDto {
    pub id: String,
    pub field: String,
    pub operator: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuleActionDto {
    pub id: String,
    pub action_type: String,
    pub action_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuleDto {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub is_active: bool,
    pub match_mode: String,
    pub priority: i64,
    pub conditions: Vec<RuleConditionDto>,
    pub actions: Vec<RuleActionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConditionInput {
    pub field: String,
    pub operator: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActionInput {
    pub action_type: String,
    pub action_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRuleBody {
    pub account_id: String,
    pub name: String,
    pub match_mode: Option<String>,
    pub conditions: Option<Vec<ConditionInput>>,
    pub actions: Option<Vec<ActionInput>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRuleBody {
    pub name: Option<String>,
    pub match_mode: Option<String>,
    pub conditions: Option<Vec<ConditionInput>>,
    pub actions: Option<Vec<ActionInput>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct EventLink {
    pub id: String,
    pub event_id: String,
    pub message_id: String,
    pub folder_id: String,
    pub subject: Option<String>,
    pub from_name: Option<String>,
    pub from_email: Option<String>,
    pub date: Option<String>,
    pub linked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddLinkBody {
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct AddLinkResponse {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct Webhook {
    pub id: String,
    pub url: String,
    #[serde(skip_serializing)]
    pub secret: Option<String>,
    pub events: String,
    pub account_id: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateWebhook {
    pub url: String,
    pub secret: Option<String>,
    pub events: Option<String>,
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWebhook {
    pub url: Option<String>,
    pub secret: Option<String>,
    pub events: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SyncEvent {
    SyncStart {
        #[serde(rename = "accountId")]
        account_id: String,
        #[serde(rename = "folderCount")]
        folder_count: Option<i64>,
    },
    SyncFolderDone {
        #[serde(rename = "accountId")]
        account_id: String,
        folder: Option<String>,
        done: Option<i64>,
        total: Option<i64>,
    },
    SyncComplete {
        #[serde(rename = "accountId")]
        account_id: String,
    },
    /// The provider refused the account's OAuth grant; the account must be
    /// reconnected (re-consent) before it syncs again.
    AccountNeedsReauth {
        #[serde(rename = "accountId")]
        account_id: String,
        reason: Option<String>,
    },
    /// A reconnect replaced the refused grant; sync resumes.
    AccountReauthed {
        #[serde(rename = "accountId")]
        account_id: String,
    },
}
