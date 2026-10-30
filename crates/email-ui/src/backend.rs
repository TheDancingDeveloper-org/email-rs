//! The `app-auth` calls the shell makes, behind a trait so tests run
//! without a server.

use crate::auth::{AuthMode, Credentials};
use email_client::{ClientCredential, EmailClient, EmailClientError};

/// A signed-in session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedIn {
    pub token: String,
    pub user_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthError {
    /// 401: wrong credentials, or a stored token that expired or was
    /// revoked. The stored token is dropped.
    Unauthorized(String),
    /// Anything else: server down, 4xx/5xx with a message. A stored token
    /// is kept, so a retry can still use it.
    Failed(String),
}

impl AuthError {
    pub fn message(&self) -> &str {
        match self {
            Self::Unauthorized(message) | Self::Failed(message) => message,
        }
    }
}

/// Blocking calls, run off the UI thread by [`crate::app::EmailApp`].
pub trait AuthApi: Send + Sync + 'static {
    /// `login` or `bootstrap`, by `credentials.mode`.
    fn sign_in(&self, credentials: &Credentials) -> Result<SignedIn, AuthError>;
    /// `GET /app-auth/me` with a stored token. Returns the user id.
    fn restore(&self, token: &str) -> Result<String, AuthError>;
    /// `POST /app-auth/logout`.
    fn sign_out(&self, token: &str) -> Result<(), AuthError>;
}

/// [`AuthApi`] over [`EmailClient`]. Each call carries its own credential
/// (`with_credential`), never the process-wide default.
pub struct HttpAuth {
    base_url: String,
    runtime: tokio::runtime::Runtime,
}

impl HttpAuth {
    pub fn new(base_url: impl Into<String>) -> std::io::Result<Self> {
        Ok(Self {
            base_url: base_url.into(),
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()?,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn client(&self, credential: ClientCredential) -> Result<EmailClient, AuthError> {
        EmailClient::new(&self.base_url)
            .map(|client| client.with_credential(credential))
            .map_err(|err| AuthError::Failed(err.to_string()))
    }
}

impl AuthApi for HttpAuth {
    fn sign_in(&self, credentials: &Credentials) -> Result<SignedIn, AuthError> {
        let client = self.client(ClientCredential::anonymous())?;
        let session = self
            .runtime
            .block_on(async {
                match credentials.mode {
                    AuthMode::SignIn => {
                        client
                            .login(&credentials.username, &credentials.password)
                            .await
                    }
                    AuthMode::FirstRun => {
                        client
                            .bootstrap(&credentials.username, &credentials.password)
                            .await
                    }
                }
            })
            .map_err(classify)?;
        let token = session
            .token
            .ok_or_else(|| AuthError::Failed("the server did not return a bearer token".into()))?;
        Ok(SignedIn {
            token,
            user_id: session.user_id,
        })
    }

    fn restore(&self, token: &str) -> Result<String, AuthError> {
        let client = self.client(ClientCredential::bearer(token))?;
        self.runtime
            .block_on(client.identity())
            .map(|identity| identity.user_id)
            .map_err(classify)
    }

    fn sign_out(&self, token: &str) -> Result<(), AuthError> {
        let client = self.client(ClientCredential::bearer(token))?;
        self.runtime
            .block_on(client.logout())
            .map(|_| ())
            .map_err(classify)
    }
}

/// Maps a client error to what the shell does with it.
pub fn classify(err: EmailClientError) -> AuthError {
    match err {
        EmailClientError::Api { status, message } if status.as_u16() == 401 => {
            AuthError::Unauthorized(message)
        }
        EmailClientError::Api { message, .. } => AuthError::Failed(message),
        EmailClientError::Request(err) => {
            AuthError::Failed(format!("could not reach the server: {err}"))
        }
        other => AuthError::Failed(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    fn api(status: StatusCode, message: &str) -> EmailClientError {
        EmailClientError::Api {
            status,
            message: message.into(),
        }
    }

    #[test]
    fn unauthorized_is_kept_apart_from_other_failures() {
        assert_eq!(
            classify(api(
                StatusCode::UNAUTHORIZED,
                "invalid username or password"
            )),
            AuthError::Unauthorized("invalid username or password".into())
        );
        assert_eq!(
            classify(api(
                StatusCode::FORBIDDEN,
                "first-run setup is loopback-only"
            )),
            AuthError::Failed("first-run setup is loopback-only".into())
        );
    }

    #[test]
    fn unreachable_server_is_a_failure() {
        // Port 9 (discard) on loopback is closed on CI hosts and dev boxes.
        let auth = HttpAuth::new("http://127.0.0.1:9").unwrap();
        let err = auth.restore("token").unwrap_err();
        assert!(matches!(err, AuthError::Failed(ref m) if m.starts_with("could not reach")));
    }
}
