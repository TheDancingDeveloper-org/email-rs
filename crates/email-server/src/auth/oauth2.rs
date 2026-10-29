#![allow(dead_code)]

use base64::Engine as _;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    pub token_type: String,
    pub scope: Option<String>,
}

/// Refresh this long before the stated expiry, so a token handed to a
/// connection outlives that connection's authentication.
pub const EXPIRY_MARGIN_SECS: i64 = 300;

/// OAuth 2.0 error response (RFC 6749 section 5.2), e.g. `invalid_grant`.
#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    error: String,
    error_description: Option<String>,
}

/// Parse a refresh-token response. A provider error such as `invalid_grant`
/// (revoked or expired grant) is an [`AppError::Auth`], which the API reports
/// as 401 and the backend as "Reconnect Gmail". A body that is neither a token
/// nor an OAuth error (an outage page, say) is a provider error, not a reason
/// to ask the user to sign in again.
fn parse_refresh_response(status: u16, body: &str) -> Result<TokenResponse> {
    if let Ok(token) = serde_json::from_str::<TokenResponse>(body) {
        return Ok(token);
    }
    if let Ok(error) = serde_json::from_str::<TokenErrorResponse>(body) {
        let description = error
            .error_description
            .map(|text| format!(": {text}"))
            .unwrap_or_default();
        return Err(AppError::Auth(format!(
            "OAuth refresh rejected ({}{description}); reconnect the account",
            error.error
        )));
    }
    let excerpt: String = body.chars().take(256).collect();
    Err(AppError::Provider(format!(
        "OAuth token endpoint returned HTTP {status} with an unreadable body ({len} bytes): {excerpt}",
        len = body.len()
    )))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix timestamp (seconds) when the access token expires.
    pub expires_at: Option<i64>,
}

impl StoredToken {
    pub fn from_token_response(resp: TokenResponse) -> Self {
        let expires_at = resp
            .expires_in
            .map(|secs| Utc::now().timestamp() + secs as i64);
        Self {
            access_token: resp.access_token,
            refresh_token: resp.refresh_token,
            expires_at,
        }
    }

    /// Returns true if the access token is expired or will expire within 5 minutes.
    pub fn is_expired(&self) -> bool {
        self.is_expired_at(Utc::now().timestamp())
    }

    /// `is_expired` against an explicit clock (Unix seconds).
    pub fn is_expired_at(&self, now: i64) -> bool {
        match self.expires_at {
            Some(exp) => now >= exp - EXPIRY_MARGIN_SECS,
            None => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub auth_url: String,
    pub token_url: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub use_pkce: bool,
    /// Extra query parameters appended to the authorization URL (e.g. Google's access_type).
    pub extra_auth_params: Vec<(String, String)>,
}

impl OAuthConfig {
    pub fn gmail(client_id: String, client_secret: String, redirect_uri: String) -> Self {
        Self {
            client_id,
            client_secret: Some(client_secret),
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            redirect_uri,
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
                "https://mail.google.com/".to_string(),
                "https://www.googleapis.com/auth/calendar".to_string(),
            ],
            use_pkce: false,
            extra_auth_params: vec![
                ("access_type".to_string(), "offline".to_string()),
                ("prompt".to_string(), "consent".to_string()),
            ],
        }
    }

    /// Microsoft / Outlook — PKCE public client, no secret required.
    pub fn outlook(client_id: String, redirect_uri: String) -> Self {
        Self {
            client_id,
            client_secret: None,
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".to_string(),
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".to_string(),
            redirect_uri,
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
                "offline_access".to_string(),
                "https://outlook.office.com/IMAP.AccessAsUser.All".to_string(),
                "https://outlook.office.com/SMTP.Send".to_string(),
                "https://graph.microsoft.com/User.Read".to_string(),
            ],
            use_pkce: true,
            extra_auth_params: vec![("prompt".to_string(), "consent".to_string())],
        }
    }

    /// Build the authorization redirect URL.
    ///
    /// Returns `(url, code_verifier)`. For PKCE flows the verifier must be
    /// stored server-side (keyed to `state`) and passed to `exchange_code`.
    pub fn authorization_url(&self, state: &str) -> (String, Option<String>) {
        let scopes = self.scopes.join(" ");
        let mut parts: Vec<(String, String)> = vec![
            ("client_id".to_string(), self.client_id.clone()),
            ("redirect_uri".to_string(), self.redirect_uri.clone()),
            ("response_type".to_string(), "code".to_string()),
            ("scope".to_string(), scopes),
            ("state".to_string(), state.to_string()),
        ];

        for (k, v) in &self.extra_auth_params {
            parts.push((k.clone(), v.clone()));
        }

        let code_verifier = if self.use_pkce {
            let (verifier, challenge) = generate_pkce_pair();
            parts.push(("code_challenge_method".to_string(), "S256".to_string()));
            parts.push(("code_challenge".to_string(), challenge));
            Some(verifier)
        } else {
            None
        };

        let query = parts
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        (format!("{}?{}", self.auth_url, query), code_verifier)
    }

    /// Exchange an authorization code for tokens.
    ///
    /// Pass `code_verifier` for PKCE flows (the value returned by `authorization_url`).
    pub async fn exchange_code(
        &self,
        code: &str,
        code_verifier: Option<&str>,
    ) -> Result<TokenResponse> {
        let client = reqwest::Client::new();
        let scope = self.scopes.join(" ");
        let mut params: Vec<(&str, &str)> = vec![
            ("code", code),
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
            ("scope", scope.as_str()),
        ];
        if let Some(secret) = &self.client_secret {
            params.push(("client_secret", secret.as_str()));
        }
        if let Some(verifier) = code_verifier {
            params.push(("code_verifier", verifier));
        }

        let body = client
            .post(&self.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| AppError::Auth(e.to_string()))?
            .text()
            .await
            .map_err(|e| AppError::Auth(e.to_string()))?;

        serde_json::from_str::<TokenResponse>(&body)
            .map_err(|e| AppError::Auth(format!("token parse error: {} — body: {}", e, body)))
    }

    pub async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse> {
        let client = reqwest::Client::new();
        let mut params: Vec<(&str, &str)> = vec![
            ("refresh_token", refresh_token),
            ("client_id", self.client_id.as_str()),
            ("grant_type", "refresh_token"),
        ];
        if let Some(secret) = &self.client_secret {
            params.push(("client_secret", secret.as_str()));
        }

        // A transport failure is transient, not a revoked grant: report it as a
        // provider error so it never reads as "reconnect the account".
        let response = client
            .post(&self.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| AppError::Provider(format!("OAuth token refresh failed: {e}")))?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|e| AppError::Provider(format!("OAuth token refresh failed: {e}")))?;

        parse_refresh_response(status, &body)
    }
}

/// Generate a PKCE (RFC 7636) code_verifier / code_challenge pair.
fn generate_pkce_pair() -> (String, String) {
    // 64 random bytes → 86-char base64url verifier (within the 43–128 char limit).
    let u1 = uuid::Uuid::new_v4();
    let u2 = uuid::Uuid::new_v4();
    let u3 = uuid::Uuid::new_v4();
    let u4 = uuid::Uuid::new_v4();
    let mut bytes = [0u8; 64];
    bytes[..16].copy_from_slice(u1.as_bytes());
    bytes[16..32].copy_from_slice(u2.as_bytes());
    bytes[32..48].copy_from_slice(u3.as_bytes());
    bytes[48..].copy_from_slice(u4.as_bytes());

    let code_verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);

    // code_challenge = BASE64URL(SHA256(ASCII(code_verifier)))
    let hash = hmac_sha256::Hash::hash(code_verifier.as_bytes());
    let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash);

    (code_verifier, code_challenge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn token_expiring_in(offset_secs: i64) -> StoredToken {
        StoredToken {
            access_token: "tok".into(),
            refresh_token: Some("ref".into()),
            expires_at: Some(Utc::now().timestamp() + offset_secs),
        }
    }

    #[test]
    fn not_expired_when_no_expiry() {
        let t = StoredToken {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: None,
        };
        assert!(!t.is_expired());
    }

    #[test]
    fn not_expired_when_far_future() {
        assert!(!token_expiring_in(3600).is_expired());
    }

    #[test]
    fn expired_when_past() {
        assert!(token_expiring_in(-1).is_expired());
    }

    #[test]
    fn expired_within_5_min_grace_window() {
        assert!(token_expiring_in(240).is_expired());
    }

    #[test]
    fn not_expired_just_outside_grace_window() {
        assert!(!token_expiring_in(360).is_expired());
    }

    #[test]
    fn from_token_response_computes_expires_at() {
        let resp = TokenResponse {
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            expires_in: Some(3600),
            token_type: "Bearer".into(),
            scope: None,
        };
        let before = Utc::now().timestamp();
        let stored = StoredToken::from_token_response(resp);
        let after = Utc::now().timestamp();
        let exp = stored.expires_at.unwrap();
        assert!(exp >= before + 3600 && exp <= after + 3600);
    }

    #[test]
    fn from_token_response_no_expiry_when_expires_in_missing() {
        let resp = TokenResponse {
            access_token: "a".into(),
            refresh_token: None,
            expires_in: None,
            token_type: "Bearer".into(),
            scope: None,
        };
        assert!(StoredToken::from_token_response(resp).expires_at.is_none());
    }

    #[test]
    fn gmail_auth_url_includes_required_params() {
        let cfg = OAuthConfig::gmail("my-cid".into(), "sec".into(), "http://localhost/cb".into());
        let (url, verifier) = cfg.authorization_url("state123");
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("client_id=my-cid"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("state=state123"));
        assert!(url.contains("prompt=consent"));
        assert!(verifier.is_none());
    }

    #[test]
    fn outlook_auth_url_includes_pkce_challenge() {
        let cfg = OAuthConfig::outlook("ms-cid".into(), "http://localhost/cb".into());
        let (url, verifier) = cfg.authorization_url("st456");
        assert!(url.starts_with("https://login.microsoftonline.com/common/oauth2/v2.0/authorize?"));
        assert!(!url.contains("access_type"));
        assert!(url.contains("client_id=ms-cid"));
        assert!(url.contains("state=st456"));
        assert!(url.contains("code_challenge="));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(verifier.is_some());
    }

    #[test]
    fn pkce_verifier_matches_challenge() {
        let cfg = OAuthConfig::outlook("id".into(), "http://localhost/cb".into());
        let (url, verifier) = cfg.authorization_url("s");
        let verifier = verifier.unwrap();
        // Extract the challenge from the URL and verify it matches SHA256(verifier).
        let challenge_param = url
            .split('&')
            .find(|p| p.contains("code_challenge="))
            .unwrap();
        let challenge = challenge_param.split('=').nth(1).unwrap();
        let hash = hmac_sha256::Hash::hash(verifier.as_bytes());
        let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash);
        assert_eq!(challenge, expected);
    }

    #[test]
    fn refresh_invalid_grant_is_a_reconnect_needed_auth_error() {
        let error = parse_refresh_response(
            400,
            r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#,
        )
        .unwrap_err();
        assert!(
            matches!(&error, AppError::Auth(message)
                if message.contains("invalid_grant") && message.contains("reconnect")),
            "{error}"
        );
    }

    #[test]
    fn refresh_outage_body_is_not_reported_as_revoked() {
        let error = parse_refresh_response(503, "<html>Service Unavailable</html>").unwrap_err();
        assert!(matches!(error, AppError::Provider(_)), "{error}");
    }

    #[test]
    fn refresh_success_parses_token() {
        let token = parse_refresh_response(
            200,
            r#"{"access_token":"new","expires_in":3599,"token_type":"Bearer"}"#,
        )
        .unwrap();
        assert_eq!(token.access_token, "new");
        assert!(token.refresh_token.is_none());
    }

    #[test]
    fn expiry_margin_is_measured_against_the_given_clock() {
        let token = StoredToken {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: Some(10_000),
        };
        assert!(!token.is_expired_at(10_000 - EXPIRY_MARGIN_SECS - 1));
        assert!(token.is_expired_at(10_000 - EXPIRY_MARGIN_SECS));
    }

    #[test]
    fn state_with_special_chars_is_url_encoded() {
        let cfg = OAuthConfig::gmail("id".into(), "sec".into(), "http://localhost/cb".into());
        let (url, _) = cfg.authorization_url("has spaces&symbols");
        assert!(!url.contains("has spaces&symbols"));
    }
}
