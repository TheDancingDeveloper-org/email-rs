use chrono::{DateTime, Utc};
use email_types::{
    Account, AccountSettings, ActionInput, AddLinkBody, AddLinkResponse, AuthClientKind,
    AuthCredentials, AuthIdentity, AuthSession, AuthorizeResponse, BulkRequest, BulkResponse,
    CalendarEvent, ConditionField, ConditionGroup, Contact, CreateAccount, CreateLabelBody,
    CreateWebhook, DraftRequest, DraftResponse, EventLink, Folder, FullMessage, Label, MatchMode,
    Message, MessageAttachment, PatchFolder, PatchMessageRequest, RuleDto, SearchResponse,
    SendRequest, SendResponse, SnoozeRequest, StatusResponse, SuggestResponse, SyncEvent,
    UpdateAccount, UpdateLabelBody, UpdateWebhook,
};
use reqwest::{Method, StatusCode, Url};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::{OnceLock, RwLock};
use thiserror::Error;

static DEFAULT_BEARER_TOKEN: OnceLock<RwLock<Option<String>>> = OnceLock::new();

pub fn set_default_bearer_token(token: Option<String>) {
    *DEFAULT_BEARER_TOKEN
        .get_or_init(|| RwLock::new(None))
        .write()
        .expect("bearer token lock") = token;
}

fn default_bearer_token() -> Option<String> {
    DEFAULT_BEARER_TOKEN
        .get_or_init(|| RwLock::new(None))
        .read()
        .expect("bearer token lock")
        .clone()
}

pub type Result<T> = std::result::Result<T, EmailClientError>;

#[derive(Debug, Error)]
pub enum EmailClientError {
    #[error("invalid base URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("API error {status}: {message}")]
    Api { status: StatusCode, message: String },
    #[error("invalid sync event: {0}")]
    SyncEvent(#[from] serde_json::Error),
}

#[derive(Clone)]
pub struct EmailClient {
    base_url: Url,
    http: reqwest::Client,
    bearer_token: Option<String>,
}

impl EmailClient {
    pub fn new(base_url: impl AsRef<str>) -> Result<Self> {
        Ok(Self {
            base_url: Url::parse(base_url.as_ref())?,
            http: reqwest::Client::new(),
            bearer_token: default_bearer_token(),
        })
    }

    pub fn with_http(base_url: impl AsRef<str>, http: reqwest::Client) -> Result<Self> {
        Ok(Self {
            base_url: Url::parse(base_url.as_ref())?,
            http,
            bearer_token: default_bearer_token(),
        })
    }

    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.bearer_token = Some(token.into());
        self
    }

    pub fn bearer_token(&self) -> Option<&str> {
        self.bearer_token.as_deref()
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<AuthSession> {
        self.auth_session(
            Method::POST,
            "app-auth/login",
            &AuthCredentials {
                username: username.to_string(),
                password: password.to_string(),
                client: AuthClientKind::Bearer,
            },
        )
        .await
    }

    pub async fn bootstrap(&self, username: &str, password: &str) -> Result<AuthSession> {
        self.auth_session(
            Method::POST,
            "app-auth/bootstrap",
            &AuthCredentials {
                username: username.to_string(),
                password: password.to_string(),
                client: AuthClientKind::Bearer,
            },
        )
        .await
    }

    pub async fn identity(&self) -> Result<AuthIdentity> {
        self.get("app-auth/me", &[]).await
    }

    pub async fn refresh_session(&self) -> Result<AuthSession> {
        self.auth_session(Method::POST, "app-auth/refresh", &serde_json::json!({}))
            .await
    }

    pub async fn logout(&self) -> Result<StatusResponse> {
        self.request(Method::POST, "app-auth/logout").await
    }

    pub fn endpoint_url(&self, path: &str) -> Url {
        self.endpoint_url_with_query(path, &[])
    }

    pub fn endpoint_url_with_query(&self, path: &str, query: &[(&str, String)]) -> Url {
        let mut url = self.base_url.clone();
        let base = url.path().trim_end_matches('/');
        let path = path.trim_start_matches('/');
        url.set_path(&format!("{base}/api/v1/{path}"));
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in query {
                pairs.append_pair(key, value);
            }
        }
        url
    }

    pub fn advanced_search_url(&self, group: &ConditionGroup) -> Option<Url> {
        let params = advanced_search_params(group);
        if params.is_empty() {
            return None;
        }
        let query = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect::<Vec<_>>();
        Some(self.endpoint_url_with_query("search", &query))
    }

    pub fn sync_events_url(&self) -> Url {
        self.endpoint_url("events")
    }

    pub async fn list_accounts(&self) -> Result<Vec<Account>> {
        self.get("accounts", &[]).await
    }

    pub async fn create_account(&self, body: &CreateAccount) -> Result<Account> {
        self.json(Method::POST, "accounts", body).await
    }

    pub async fn update_account(&self, id: &str, body: &UpdateAccount) -> Result<Account> {
        self.json(Method::PATCH, &format!("accounts/{id}"), body)
            .await
    }

    pub async fn delete_account(&self, id: &str) -> Result<serde_json::Value> {
        self.request(Method::DELETE, &format!("accounts/{id}"))
            .await
    }

    pub async fn account_settings(&self, id: &str) -> Result<AccountSettings> {
        self.get(&format!("accounts/{id}/settings"), &[]).await
    }

    pub async fn list_folders(&self, account_id: &str) -> Result<Vec<Folder>> {
        self.get(&format!("accounts/{account_id}/folders"), &[])
            .await
    }

    pub async fn patch_folder(
        &self,
        folder_id: &str,
        body: &PatchFolder,
    ) -> Result<serde_json::Value> {
        self.json(Method::PATCH, &format!("folders/{folder_id}"), body)
            .await
    }

    pub async fn mark_folder_read(&self, folder_id: &str) -> Result<serde_json::Value> {
        self.request(Method::POST, &format!("folders/{folder_id}/mark-read"))
            .await
    }

    pub async fn list_folder_messages(&self, folder_id: &str) -> Result<Vec<Message>> {
        self.get(&format!("folders/{folder_id}/messages"), &[])
            .await
    }

    pub async fn list_label_messages(&self, label_id: &str) -> Result<Vec<Message>> {
        self.get(&format!("labels/{label_id}/messages"), &[]).await
    }

    pub async fn list_smart_messages(&self, kind: &str) -> Result<Vec<Message>> {
        self.get(&format!("smart-folders/{kind}/messages"), &[])
            .await
    }

    pub async fn search(&self, query: &str, account_id: Option<&str>) -> Result<SearchResponse> {
        let mut params = vec![("q", query.to_string())];
        if let Some(account_id) = account_id {
            params.push(("account_id", account_id.to_string()));
        }
        self.get("search", &params).await
    }

    pub async fn advanced_search(&self, group: &ConditionGroup) -> Result<SearchResponse> {
        let params = advanced_search_params(group);
        let query = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect::<Vec<_>>();
        self.get("search", &query).await
    }

    pub async fn suggest(&self, query: &str, account_id: Option<&str>) -> Result<SuggestResponse> {
        let mut params = vec![("q", query.to_string())];
        if let Some(account_id) = account_id {
            params.push(("account_id", account_id.to_string()));
        }
        self.get("search/suggest", &params).await
    }

    pub async fn suggest_contacts(&self, query: &str) -> Result<Vec<Contact>> {
        self.get("contacts/suggest", &[("q", query.to_string())])
            .await
    }

    pub async fn get_message(&self, message_id: &str) -> Result<FullMessage> {
        self.get(&format!("messages/{message_id}"), &[]).await
    }

    pub async fn list_attachments(&self, message_id: &str) -> Result<Vec<MessageAttachment>> {
        self.get(&format!("messages/{message_id}/attachments"), &[])
            .await
    }

    pub async fn download_attachment(
        &self,
        message_id: &str,
        attachment_id: &str,
    ) -> Result<Vec<u8>> {
        self.bytes(&format!(
            "messages/{message_id}/attachments/{attachment_id}"
        ))
        .await
    }

    pub async fn patch_message(
        &self,
        message_id: &str,
        body: &PatchMessageRequest,
    ) -> Result<serde_json::Value> {
        self.json(Method::PATCH, &format!("messages/{message_id}"), body)
            .await
    }

    pub async fn delete_message(&self, message_id: &str) -> Result<serde_json::Value> {
        self.request(Method::DELETE, &format!("messages/{message_id}"))
            .await
    }

    pub async fn archive_message(&self, message_id: &str) -> Result<serde_json::Value> {
        self.request(Method::POST, &format!("messages/{message_id}/archive"))
            .await
    }

    pub async fn bulk_messages(&self, body: &BulkRequest) -> Result<BulkResponse> {
        self.json(Method::POST, "messages/bulk", body).await
    }

    pub async fn send_message(&self, body: &SendRequest) -> Result<SendResponse> {
        self.json(Method::POST, "messages", body).await
    }

    pub async fn create_draft(&self, body: &DraftRequest) -> Result<DraftResponse> {
        self.json(Method::POST, "drafts", body).await
    }

    pub async fn update_draft(&self, id: &str, body: &DraftRequest) -> Result<DraftResponse> {
        self.json(Method::PUT, &format!("drafts/{id}"), body).await
    }

    pub async fn delete_draft(&self, id: &str) -> Result<serde_json::Value> {
        self.request(Method::DELETE, &format!("drafts/{id}")).await
    }

    pub async fn trigger_sync(&self, account_id: &str) -> Result<serde_json::Value> {
        self.request(Method::POST, &format!("sync/{account_id}"))
            .await
    }

    pub async fn snooze_message(&self, message_id: &str, body: &SnoozeRequest) -> Result<()> {
        self.json_empty(Method::POST, &format!("messages/{message_id}/snooze"), body)
            .await
    }

    pub async fn unsnooze_message(&self, message_id: &str) -> Result<()> {
        self.empty(Method::DELETE, &format!("messages/{message_id}/snooze"))
            .await
    }

    pub async fn get_message_labels(&self, message_id: &str) -> Result<Vec<Label>> {
        self.get(&format!("messages/{message_id}/labels"), &[])
            .await
    }

    pub async fn add_message_label(&self, message_id: &str, label_id: &str) -> Result<()> {
        self.empty(
            Method::POST,
            &format!("messages/{message_id}/labels/{label_id}"),
        )
        .await
    }

    pub async fn remove_message_label(&self, message_id: &str, label_id: &str) -> Result<()> {
        self.empty(
            Method::DELETE,
            &format!("messages/{message_id}/labels/{label_id}"),
        )
        .await
    }

    pub async fn list_labels(&self, account_id: &str) -> Result<Vec<Label>> {
        self.get("labels", &[("account_id", account_id.to_string())])
            .await
    }

    pub async fn create_label(&self, body: &CreateLabelBody) -> Result<Label> {
        self.json(Method::POST, "labels", body).await
    }

    pub async fn update_label(&self, label_id: &str, body: &UpdateLabelBody) -> Result<Label> {
        self.json(Method::PUT, &format!("labels/{label_id}"), body)
            .await
    }

    pub async fn delete_label(&self, label_id: &str) -> Result<()> {
        self.empty(Method::DELETE, &format!("labels/{label_id}"))
            .await
    }

    pub async fn list_events(
        &self,
        account_id: Option<&str>,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> Result<Vec<CalendarEvent>> {
        let mut params = Vec::new();
        if let Some(account_id) = account_id {
            params.push(("account_id", account_id.to_string()));
        }
        if let Some(from) = from {
            params.push(("from", from.to_rfc3339()));
        }
        if let Some(to) = to {
            params.push(("to", to.to_rfc3339()));
        }
        self.get("calendar/events", &params).await
    }

    pub async fn get_event(&self, event_id: &str) -> Result<CalendarEvent> {
        self.get(&format!("calendar/events/{event_id}"), &[]).await
    }

    pub async fn list_event_links(&self, event_id: &str) -> Result<Vec<EventLink>> {
        self.get(&format!("calendar/events/{event_id}/links"), &[])
            .await
    }

    pub async fn add_event_link(
        &self,
        event_id: &str,
        body: &AddLinkBody,
    ) -> Result<AddLinkResponse> {
        self.json(
            Method::POST,
            &format!("calendar/events/{event_id}/links"),
            body,
        )
        .await
    }

    pub async fn remove_event_link(&self, event_id: &str, message_id: &str) -> Result<()> {
        self.empty(
            Method::DELETE,
            &format!("calendar/events/{event_id}/links/{message_id}"),
        )
        .await
    }

    pub async fn list_rules(&self, account_id: &str) -> Result<Vec<RuleDto>> {
        self.get("rules", &[("account_id", account_id.to_string())])
            .await
    }

    pub async fn create_rule(
        &self,
        account_id: &str,
        name: &str,
        match_mode: Option<&str>,
        conditions: Option<Vec<email_types::ConditionInput>>,
        actions: Option<Vec<ActionInput>>,
    ) -> Result<RuleDto> {
        let body = email_types::CreateRuleBody {
            account_id: account_id.to_string(),
            name: name.to_string(),
            match_mode: match_mode.map(ToString::to_string),
            conditions,
            actions,
        };
        self.json(Method::POST, "rules", &body).await
    }

    pub async fn update_rule(
        &self,
        rule_id: &str,
        body: &email_types::UpdateRuleBody,
    ) -> Result<RuleDto> {
        self.json(Method::PUT, &format!("rules/{rule_id}"), body)
            .await
    }

    pub async fn delete_rule(&self, rule_id: &str) -> Result<serde_json::Value> {
        self.request(Method::DELETE, &format!("rules/{rule_id}"))
            .await
    }

    pub async fn toggle_rule(&self, rule_id: &str) -> Result<RuleDto> {
        self.request(Method::POST, &format!("rules/{rule_id}/toggle"))
            .await
    }

    pub async fn list_webhooks(&self) -> Result<Vec<email_types::Webhook>> {
        self.get("webhooks", &[]).await
    }

    pub async fn create_webhook(&self, body: &CreateWebhook) -> Result<email_types::Webhook> {
        self.json(Method::POST, "webhooks", body).await
    }

    pub async fn update_webhook(
        &self,
        webhook_id: &str,
        body: &UpdateWebhook,
    ) -> Result<email_types::Webhook> {
        self.json(Method::PATCH, &format!("webhooks/{webhook_id}"), body)
            .await
    }

    pub async fn delete_webhook(&self, webhook_id: &str) -> Result<serde_json::Value> {
        self.request(Method::DELETE, &format!("webhooks/{webhook_id}"))
            .await
    }

    pub async fn gmail_authorize(&self) -> Result<AuthorizeResponse> {
        self.get("auth/gmail/authorize", &[]).await
    }

    pub async fn microsoft_authorize(&self) -> Result<AuthorizeResponse> {
        self.get("auth/microsoft/authorize", &[]).await
    }

    pub async fn open_sync_events(&self) -> Result<reqwest::Response> {
        let response = self
            .authorized(self.http.get(self.sync_events_url()))
            .send()
            .await?;
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(api_error(response.status(), response.bytes().await?))
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        let url = self.endpoint_url_with_query(path, query);
        let response = self.authorized(self.http.get(url)).send().await?;
        decode(response).await
    }

    async fn request<T: DeserializeOwned>(&self, method: Method, path: &str) -> Result<T> {
        let response = self
            .authorized(self.http.request(method, self.endpoint_url(path)))
            .send()
            .await?;
        decode(response).await
    }

    async fn json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: &impl Serialize,
    ) -> Result<T> {
        let response = self
            .authorized(self.http.request(method, self.endpoint_url(path)))
            .json(body)
            .send()
            .await?;
        decode(response).await
    }

    async fn json_empty(&self, method: Method, path: &str, body: &impl Serialize) -> Result<()> {
        let response = self
            .authorized(self.http.request(method, self.endpoint_url(path)))
            .json(body)
            .send()
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(api_error(response.status(), response.bytes().await?))
        }
    }

    async fn empty(&self, method: Method, path: &str) -> Result<()> {
        let response = self
            .authorized(self.http.request(method, self.endpoint_url(path)))
            .send()
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(api_error(response.status(), response.bytes().await?))
        }
    }

    async fn bytes(&self, path: &str) -> Result<Vec<u8>> {
        let response = self
            .authorized(self.http.get(self.endpoint_url(path)))
            .send()
            .await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        if status.is_success() {
            Ok(bytes.to_vec())
        } else {
            Err(api_error(status, bytes))
        }
    }

    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.bearer_token.as_deref() {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    async fn auth_session(
        &self,
        method: Method,
        path: &str,
        body: &impl Serialize,
    ) -> Result<AuthSession> {
        self.json(method, path, body).await
    }
}

async fn decode<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
    let status = response.status();
    if status.is_success() {
        Ok(response.json::<T>().await?)
    } else {
        Err(api_error(status, response.bytes().await?))
    }
}

fn api_error(status: StatusCode, body: impl AsRef<[u8]>) -> EmailClientError {
    let message = serde_json::from_slice::<serde_json::Value>(body.as_ref())
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.as_str())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| String::from_utf8_lossy(body.as_ref()).into_owned());
    EmailClientError::Api { status, message }
}

pub fn parse_sync_event(data: &str) -> Result<SyncEvent> {
    Ok(serde_json::from_str(data)?)
}

pub fn advanced_search_params(group: &ConditionGroup) -> Vec<(String, String)> {
    let active = group
        .conditions
        .iter()
        .filter(|condition| !condition.value.trim().is_empty())
        .collect::<Vec<_>>();
    if active.is_empty() {
        return Vec::new();
    }

    let mut params = vec![("match".to_string(), match_mode_param(&group.match_mode))];
    let text_values = active
        .iter()
        .filter(|condition| is_text_field(&condition.field))
        .map(|condition| condition.value.trim().to_string())
        .collect::<Vec<_>>();
    if !text_values.is_empty() {
        params.push(("q".to_string(), text_values.join(" ")));
    }

    for condition in active {
        let field = field_param(&condition.field);
        params.push((field.clone(), condition.value.trim().to_string()));
        if is_text_field(&condition.field)
            && !matches!(condition.operator, email_types::ConditionOperator::Contains)
        {
            params.push((format!("{field}_op"), operator_param(&condition.operator)));
        }
    }

    params
}

fn is_text_field(field: &ConditionField) -> bool {
    matches!(
        field,
        ConditionField::From
            | ConditionField::To
            | ConditionField::Cc
            | ConditionField::Subject
            | ConditionField::Body
            | ConditionField::Filename
    )
}

fn field_param(field: &ConditionField) -> String {
    serde_json::from_value::<String>(serde_json::to_value(field).expect("field serializes"))
        .expect("field serializes as string")
}

fn operator_param(operator: &email_types::ConditionOperator) -> String {
    serde_json::from_value::<String>(serde_json::to_value(operator).expect("operator serializes"))
        .expect("operator serializes as string")
}

fn match_mode_param(mode: &MatchMode) -> String {
    serde_json::from_value::<String>(serde_json::to_value(mode).expect("match mode serializes"))
        .expect("match mode serializes as string")
}

#[cfg(test)]
mod tests {
    use super::*;
    use email_types::{Condition, ConditionOperator, CreateAccount};

    #[test]
    fn endpoint_url_joins_base_and_api_prefix() {
        let client = EmailClient::new("http://127.0.0.1:8585/").unwrap();
        assert_eq!(
            client.endpoint_url("accounts").as_str(),
            "http://127.0.0.1:8585/api/v1/accounts"
        );
    }

    #[test]
    fn endpoint_url_preserves_base_path_and_query_encoding() {
        let client = EmailClient::new("http://localhost:8585/root").unwrap();
        let url = client.endpoint_url_with_query(
            "search",
            &[("q", "hello world".into()), ("account_id", "acc/1".into())],
        );
        assert_eq!(
            url.as_str(),
            "http://localhost:8585/root/api/v1/search?q=hello+world&account_id=acc%2F1"
        );
    }

    #[test]
    fn bearer_token_is_attached_to_every_request_builder() {
        let client = EmailClient::new("https://mail.example.test")
            .unwrap()
            .with_bearer_token("test-token");
        let request = client
            .authorized(client.http.get(client.endpoint_url("accounts")))
            .build()
            .unwrap();
        assert_eq!(
            request.headers()[reqwest::header::AUTHORIZATION],
            "Bearer test-token"
        );
    }

    #[test]
    fn unauthenticated_client_does_not_invent_an_authorization_header() {
        let client = EmailClient::new("https://mail.example.test").unwrap();
        let request = client
            .authorized(client.http.get(client.endpoint_url("accounts")))
            .build()
            .unwrap();
        assert!(!request
            .headers()
            .contains_key(reqwest::header::AUTHORIZATION));
    }

    #[test]
    fn attachment_download_url_uses_message_attachment_route() {
        let client = EmailClient::new("http://localhost:8585").unwrap();
        assert_eq!(
            client
                .endpoint_url("messages/msg-1/attachments/att-1")
                .as_str(),
            "http://localhost:8585/api/v1/messages/msg-1/attachments/att-1"
        );
    }

    #[test]
    fn create_account_json_uses_camel_case() {
        let body = CreateAccount {
            name: "Fixture".into(),
            email: "fixture@example.test".into(),
            provider_type: "generic_imap".into(),
            auth_type: "basic".into(),
            oauth_token_json: None,
            host: Some("imap.example.test".into()),
            port: Some(993),
            use_ssl: Some(true),
            password: Some("secret".into()),
            smtp_host: Some("smtp.example.test".into()),
            smtp_port: Some(587),
            smtp_password: Some("secret".into()),
        };
        let json = serde_json::to_value(body).unwrap();
        assert_eq!(json["providerType"], "generic_imap");
        assert_eq!(json["authType"], "basic");
        assert_eq!(json["smtpHost"], "smtp.example.test");
        assert!(json.get("provider_type").is_none());
    }

    #[test]
    fn api_error_prefers_error_key() {
        let err = api_error(
            StatusCode::BAD_REQUEST,
            br#"{"error":"structured failure"}"#,
        );
        assert!(
            matches!(err, EmailClientError::Api { message, .. } if message == "structured failure")
        );
    }

    #[test]
    fn advanced_search_params_match_frontend_url_shape() {
        let group = ConditionGroup {
            match_mode: MatchMode::Any,
            conditions: vec![
                Condition {
                    id: "c1".into(),
                    field: ConditionField::From,
                    operator: ConditionOperator::StartsWith,
                    value: "alex".into(),
                },
                Condition {
                    id: "c2".into(),
                    field: ConditionField::HasAttachment,
                    operator: ConditionOperator::Equals,
                    value: "true".into(),
                },
            ],
        };

        assert_eq!(
            advanced_search_params(&group),
            vec![
                ("match".into(), "any".into()),
                ("q".into(), "alex".into()),
                ("from".into(), "alex".into()),
                ("from_op".into(), "starts_with".into()),
                ("has_attachment".into(), "true".into()),
            ]
        );
    }

    #[test]
    fn empty_advanced_search_has_no_url() {
        let client = EmailClient::new("http://localhost:8585").unwrap();
        let group = ConditionGroup {
            match_mode: MatchMode::All,
            conditions: vec![Condition {
                id: "c1".into(),
                field: ConditionField::Subject,
                operator: ConditionOperator::Contains,
                value: " ".into(),
            }],
        };
        assert!(client.advanced_search_url(&group).is_none());
    }

    #[test]
    fn parses_sync_event_payload() {
        let event = parse_sync_event(
            r#"{"type":"sync_folder_done","accountId":"acc1","folder":"INBOX","done":1,"total":3}"#,
        )
        .unwrap();
        assert_eq!(
            event,
            SyncEvent::SyncFolderDone {
                account_id: "acc1".into(),
                folder: Some("INBOX".into()),
                done: Some(1),
                total: Some(3),
            }
        );
    }
}
