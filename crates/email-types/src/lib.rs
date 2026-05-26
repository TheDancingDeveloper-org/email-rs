use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub email: String,
    pub provider_type: String,
    pub auth_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub full_path: String,
    pub special_use: Option<String>,
    pub unread_count: i64,
    pub total_count: i64,
    pub synced_at: Option<String>,
    pub is_excluded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PatchFolder {
    pub is_excluded: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MessageBody {
    pub html_body: Option<String>,
    pub text_body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FullMessage {
    pub id: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BulkAction {
    Archive,
    Delete,
    MarkRead,
    MarkUnread,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkRequest {
    pub ids: Vec<String>,
    pub action: BulkAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PatchMessageRequest {
    pub is_read: Option<bool>,
    pub is_flagged: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnoozeRequest {
    pub until: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendResponse {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizeResponse {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateLabelBody {
    pub account_id: String,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLabelBody {
    pub name: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Attendee {
    pub email: String,
    pub name: Option<String>,
    pub response_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct CalendarSuggestion {
    pub id: String,
    pub title: String,
    pub start_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuggestResponse {
    pub messages: Vec<MessageSuggestion>,
    pub events: Vec<CalendarSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResponse {
    pub messages: Vec<Message>,
    pub events: Vec<CalendarSearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOperator {
    Contains,
    NotContains,
    Equals,
    StartsWith,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Condition {
    pub id: String,
    pub field: ConditionField,
    pub operator: ConditionOperator,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MatchMode {
    All,
    Any,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConditionGroup {
    #[serde(rename = "match")]
    pub match_mode: MatchMode,
    pub conditions: Vec<Condition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuleConditionDto {
    pub id: String,
    pub field: String,
    pub operator: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuleActionDto {
    pub id: String,
    pub action_type: String,
    pub action_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConditionInput {
    pub field: String,
    pub operator: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActionInput {
    pub action_type: String,
    pub action_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateRuleBody {
    pub account_id: String,
    pub name: String,
    pub match_mode: Option<String>,
    pub conditions: Option<Vec<ConditionInput>>,
    pub actions: Option<Vec<ActionInput>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRuleBody {
    pub name: Option<String>,
    pub match_mode: Option<String>,
    pub conditions: Option<Vec<ConditionInput>>,
    pub actions: Option<Vec<ActionInput>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddLinkBody {
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddLinkResponse {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
#[serde(rename_all = "camelCase")]
pub struct Webhook {
    pub id: String,
    pub url: String,
    pub secret: Option<String>,
    pub events: String,
    pub account_id: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateWebhook {
    pub url: String,
    pub secret: Option<String>,
    pub events: Option<String>,
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWebhook {
    pub url: Option<String>,
    pub secret: Option<String>,
    pub events: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
}
