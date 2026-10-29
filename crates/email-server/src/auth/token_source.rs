//! Per-connection OAuth access tokens for long-running mail operations.
//!
//! A sync run opens hundreds of short IMAP connections and can outlive the
//! access token it started with. [`AccessTokenSource`] hands every connection
//! a token that is not about to expire, refreshing it at most once per account
//! at a time (a per-account async mutex shared by every provider instance), and
//! [`with_fresh_token`] retries an operation once with a force-refreshed token
//! when the server rejects the credentials. A refresh the provider itself
//! refuses (`invalid_grant`: the grant was revoked) surfaces as
//! [`AppError::Auth`], which callers map to "reconnect the account".

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};

use async_trait::async_trait;
use tokio::sync::{Mutex, OwnedMutexGuard};
use tracing::{info, warn};

use super::oauth2::StoredToken;
use crate::error::{AppError, Result};

/// Seconds since the Unix epoch. Injectable so tests can cross token expiry.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

pub fn system_clock() -> Clock {
    Arc::new(|| chrono::Utc::now().timestamp())
}

/// Where an account's OAuth token is persisted and how it is refreshed.
#[async_trait]
pub trait TokenStore: Send + Sync {
    /// The currently persisted token (another task may have refreshed it).
    async fn load(&self) -> Result<StoredToken>;
    /// Exchange the refresh token for a new access token and persist it.
    async fn refresh(&self, current: &StoredToken) -> Result<StoredToken>;
}

static REFRESH_LOCKS: OnceLock<StdMutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();

/// Serialize token refreshes for one account across every task and provider
/// instance in this process, so parallel folder workers (and an API request
/// running alongside a sync) never refresh the same account concurrently.
pub async fn lock_account_refresh(account_id: &str) -> OwnedMutexGuard<()> {
    let lock = {
        let mut locks = REFRESH_LOCKS
            .get_or_init(|| StdMutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        locks.entry(account_id.to_string()).or_default().clone()
    };
    lock.lock_owned().await
}

/// Supplies a usable access token for each new connection of one account.
pub struct AccessTokenSource {
    account_id: String,
    store: Arc<dyn TokenStore>,
    cached: StdMutex<StoredToken>,
    /// A refresh the provider refused (revoked grant), keyed by the refresh
    /// token it was attempted with, so parallel workers report it instead of
    /// each asking the provider again.
    refused: StdMutex<Option<(Option<String>, String)>>,
    clock: Clock,
}

impl AccessTokenSource {
    pub fn new(account_id: String, initial: StoredToken, store: Arc<dyn TokenStore>) -> Self {
        Self::with_clock(account_id, initial, store, system_clock())
    }

    pub fn with_clock(
        account_id: String,
        initial: StoredToken,
        store: Arc<dyn TokenStore>,
        clock: Clock,
    ) -> Self {
        Self {
            account_id,
            store,
            cached: StdMutex::new(initial),
            refused: StdMutex::new(None),
            clock,
        }
    }

    /// Refresh `current` through the store, at most once per refresh token
    /// when the provider refuses it. Callers hold the account refresh lock.
    async fn refresh_locked(&self, current: &StoredToken) -> Result<StoredToken> {
        if let Some((refresh_token, message)) = self
            .refused
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
        {
            if *refresh_token == current.refresh_token {
                return Err(AppError::Auth(message.clone()));
            }
        }
        match self.store.refresh(current).await {
            Ok(token) => Ok(token),
            Err(AppError::Auth(message)) => {
                *self
                    .refused
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    Some((current.refresh_token.clone(), message.clone()));
                Err(AppError::Auth(message))
            }
            Err(error) => Err(error),
        }
    }

    fn cached(&self) -> StoredToken {
        self.cached
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn remember(&self, token: &StoredToken) {
        *self
            .cached
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = token.clone();
    }

    fn usable(&self, token: &StoredToken) -> bool {
        !token.access_token.is_empty() && !token.is_expired_at((self.clock)())
    }

    /// The access token as currently cached, without refreshing.
    pub fn peek(&self) -> String {
        self.cached().access_token
    }

    /// An access token with more than the expiry margin left, refreshing it
    /// first when it is close to expiry.
    pub async fn access_token(&self) -> Result<String> {
        let cached = self.cached();
        if self.usable(&cached) {
            return Ok(cached.access_token);
        }
        let _guard = lock_account_refresh(&self.account_id).await;
        // A parallel worker may have refreshed while this one waited.
        let cached = self.cached();
        if self.usable(&cached) {
            return Ok(cached.access_token);
        }
        let persisted = self.store.load().await?;
        let token = if self.usable(&persisted) {
            persisted
        } else {
            info!(
                "OAuth access token for account {} is near expiry; refreshing",
                self.account_id
            );
            self.refresh_locked(&persisted).await?
        };
        self.remember(&token);
        Ok(token.access_token)
    }

    /// The server rejected `rejected`: return a newer token, refreshing unless
    /// a parallel worker already replaced the rejected one.
    pub async fn refresh_after_rejection(&self, rejected: &str) -> Result<String> {
        let _guard = lock_account_refresh(&self.account_id).await;
        let cached = self.cached();
        if cached.access_token != rejected && self.usable(&cached) {
            return Ok(cached.access_token);
        }
        let persisted = self.store.load().await?;
        let token = if persisted.access_token != rejected && self.usable(&persisted) {
            persisted
        } else {
            warn!(
                "OAuth access token for account {} was rejected; forcing a refresh",
                self.account_id
            );
            self.refresh_locked(&persisted).await?
        };
        self.remember(&token);
        Ok(token.access_token)
    }
}

/// Run `op` with a fresh access token. If the server rejects the credentials
/// (`is_rejection`), force one refresh and run `op` once more; a second
/// rejection of a just-refreshed token is reported as [`AppError::Auth`].
pub async fn with_fresh_token<T, F, Fut>(
    source: &AccessTokenSource,
    is_rejection: impl Fn(&AppError) -> bool,
    mut op: F,
) -> Result<T>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let token = source.access_token().await?;
    match op(token.clone()).await {
        Err(error) if is_rejection(&error) => {
            warn!(
                "credentials rejected for account {} ({}); refreshing and retrying once",
                source.account_id, error
            );
            let fresh = source.refresh_after_rejection(&token).await?;
            op(fresh).await.map_err(|error| {
                if is_rejection(&error) {
                    AppError::Auth(format!(
                        "the mail server rejected a freshly refreshed OAuth token ({error}); \
                         reconnect the account"
                    ))
                } else {
                    error
                }
            })
        }
        other => other,
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! Fake clock, token store and XOAUTH2 server shared by the token tests
    //! here and the sync tests in `imap::sync`.

    use std::collections::HashSet;
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

    use super::*;

    #[derive(Clone)]
    pub struct FakeClock(pub Arc<AtomicI64>);

    impl FakeClock {
        pub fn new(start: i64) -> Self {
            Self(Arc::new(AtomicI64::new(start)))
        }
        pub fn now(&self) -> i64 {
            self.0.load(Ordering::SeqCst)
        }
        pub fn advance(&self, secs: i64) {
            self.0.fetch_add(secs, Ordering::SeqCst);
        }
        pub fn clock(&self) -> Clock {
            let now = self.0.clone();
            Arc::new(move || now.load(Ordering::SeqCst))
        }
    }

    /// Issues `token-N` access tokens valid for `lifetime` seconds of fake time,
    /// and plays the IMAP server: it accepts only unexpired, unrevoked tokens.
    pub struct FakeOAuth {
        pub clock: FakeClock,
        pub lifetime: i64,
        pub refreshes: AtomicUsize,
        /// XOAUTH2 attempts, and how many of them the server rejected.
        pub auth_calls: AtomicUsize,
        pub rejections: AtomicUsize,
        pub revoked_grant: std::sync::atomic::AtomicBool,
        persisted: StdMutex<StoredToken>,
        issued: StdMutex<HashMap<String, i64>>,
        revoked_tokens: StdMutex<HashSet<String>>,
    }

    impl FakeOAuth {
        pub fn new(clock: FakeClock, lifetime: i64) -> Arc<Self> {
            let first = StoredToken {
                access_token: "token-0".into(),
                refresh_token: Some("refresh".into()),
                expires_at: Some(clock.now() + lifetime),
            };
            Arc::new(Self {
                issued: StdMutex::new(HashMap::from([(
                    first.access_token.clone(),
                    first.expires_at.unwrap(),
                )])),
                persisted: StdMutex::new(first),
                clock,
                lifetime,
                refreshes: AtomicUsize::new(0),
                auth_calls: AtomicUsize::new(0),
                rejections: AtomicUsize::new(0),
                revoked_grant: Default::default(),
                revoked_tokens: Default::default(),
            })
        }

        pub fn current(&self) -> StoredToken {
            self.persisted.lock().unwrap().clone()
        }

        pub fn refresh_count(&self) -> usize {
            self.refreshes.load(Ordering::SeqCst)
        }

        /// Invalidate an access token before its stated expiry, as Google may.
        pub fn revoke_token(&self, token: &str) {
            self.revoked_tokens
                .lock()
                .unwrap()
                .insert(token.to_string());
        }

        /// XOAUTH2 against the fake server; the error mirrors Gmail's reply.
        pub fn authenticate(&self, token: &str) -> Result<()> {
            self.auth_calls.fetch_add(1, Ordering::SeqCst);
            let valid = self
                .issued
                .lock()
                .unwrap()
                .get(token)
                .is_some_and(|expires| self.clock.now() < *expires)
                && !self.revoked_tokens.lock().unwrap().contains(token);
            if valid {
                Ok(())
            } else {
                self.rejections.fetch_add(1, Ordering::SeqCst);
                Err(AppError::Imap(
                    "IMAP auth failed: no response: [AUTHENTICATIONFAILED] Invalid credentials (Failure)"
                        .into(),
                ))
            }
        }

        pub fn source(self: &Arc<Self>, account_id: &str) -> AccessTokenSource {
            AccessTokenSource::with_clock(
                account_id.to_string(),
                self.current(),
                self.clone(),
                self.clock.clock(),
            )
        }
    }

    #[async_trait]
    impl TokenStore for FakeOAuth {
        async fn load(&self) -> Result<StoredToken> {
            Ok(self.current())
        }

        async fn refresh(&self, _current: &StoredToken) -> Result<StoredToken> {
            let n = self.refreshes.fetch_add(1, Ordering::SeqCst) + 1;
            // Let parallel callers pile up on the refresh lock.
            tokio::task::yield_now().await;
            if self.revoked_grant.load(Ordering::SeqCst) {
                return Err(AppError::Auth(
                    "OAuth refresh rejected (invalid_grant: Token has been expired or revoked.); \
                     reconnect the account"
                        .into(),
                ));
            }
            let token = StoredToken {
                access_token: format!("token-{n}"),
                refresh_token: Some("refresh".into()),
                expires_at: Some(self.clock.now() + self.lifetime),
            };
            self.issued
                .lock()
                .unwrap()
                .insert(token.access_token.clone(), token.expires_at.unwrap());
            *self.persisted.lock().unwrap() = token.clone();
            Ok(token)
        }
    }

    pub fn is_imap_rejection(error: &AppError) -> bool {
        matches!(error, AppError::Imap(message) if message.contains("AUTHENTICATIONFAILED"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::testing::*;
    use super::*;

    fn account() -> String {
        format!("acct-{}", uuid::Uuid::new_v4())
    }

    #[tokio::test]
    async fn connections_after_expiry_get_a_refreshed_token() {
        let clock = FakeClock::new(1_000_000);
        let oauth = FakeOAuth::new(clock.clone(), 3600);
        let source = Arc::new(oauth.source(&account()));

        // Connections spread over 2.5 token lifetimes, four at a time.
        for _ in 0..90 {
            let mut workers = Vec::new();
            for _ in 0..4 {
                let source = source.clone();
                let oauth = oauth.clone();
                workers.push(tokio::spawn(async move {
                    with_fresh_token(&source, is_imap_rejection, |token| {
                        let oauth = oauth.clone();
                        async move { oauth.authenticate(&token) }
                    })
                    .await
                }));
            }
            for worker in workers {
                worker.await.unwrap().unwrap();
            }
            clock.advance(100);
        }
        // Proactive refresh at ~t+3300 and ~t+6600: one per crossing, never one
        // per parallel worker.
        assert_eq!(oauth.refresh_count(), 2);
    }

    #[tokio::test]
    async fn a_rejected_token_is_refreshed_and_retried_exactly_once() {
        let clock = FakeClock::new(1_000_000);
        let oauth = FakeOAuth::new(clock.clone(), 3600);
        let source = oauth.source(&account());
        oauth.revoke_token("token-0");

        let attempts = AtomicUsize::new(0);
        let used = StdMutex::new(Vec::new());
        with_fresh_token(&source, is_imap_rejection, |token| {
            attempts.fetch_add(1, Ordering::SeqCst);
            used.lock().unwrap().push(token.clone());
            let result = oauth.authenticate(&token);
            async move { result }
        })
        .await
        .unwrap();

        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(oauth.refresh_count(), 1);
        assert_eq!(*used.lock().unwrap(), vec!["token-0", "token-1"]);
    }

    #[tokio::test]
    async fn parallel_rejections_share_one_refresh() {
        let clock = FakeClock::new(1_000_000);
        let oauth = FakeOAuth::new(clock.clone(), 3600);
        let source = Arc::new(oauth.source(&account()));
        oauth.revoke_token("token-0");

        let mut workers = Vec::new();
        for _ in 0..4 {
            let source = source.clone();
            let oauth = oauth.clone();
            workers.push(tokio::spawn(async move {
                with_fresh_token(&source, is_imap_rejection, |token| {
                    let result = oauth.authenticate(&token);
                    async move { result }
                })
                .await
            }));
        }
        for worker in workers {
            worker.await.unwrap().unwrap();
        }
        assert_eq!(oauth.refresh_count(), 1);
    }

    #[tokio::test]
    async fn a_revoked_grant_reports_reconnect_needed_without_looping() {
        let clock = FakeClock::new(1_000_000);
        let oauth = FakeOAuth::new(clock.clone(), 3600);
        let source = oauth.source(&account());
        oauth.revoke_token("token-0");
        oauth
            .revoked_grant
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let attempts = AtomicUsize::new(0);
        let error = with_fresh_token(&source, is_imap_rejection, |token| {
            attempts.fetch_add(1, Ordering::SeqCst);
            let result = oauth.authenticate(&token);
            async move { result }
        })
        .await
        .unwrap_err();

        assert!(
            matches!(&error, AppError::Auth(message) if message.contains("invalid_grant")),
            "{error}"
        );
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert_eq!(oauth.refresh_count(), 1);
    }

    #[tokio::test]
    async fn a_fresh_token_rejected_again_is_an_auth_error_not_a_loop() {
        let clock = FakeClock::new(1_000_000);
        let oauth = FakeOAuth::new(clock.clone(), 3600);
        let source = oauth.source(&account());

        let attempts = AtomicUsize::new(0);
        let error = with_fresh_token(&source, is_imap_rejection, |_token| {
            attempts.fetch_add(1, Ordering::SeqCst);
            async {
                Err::<(), _>(AppError::Imap(
                    "IMAP auth failed: [AUTHENTICATIONFAILED] Invalid credentials".into(),
                ))
            }
        })
        .await
        .unwrap_err();

        assert!(matches!(error, AppError::Auth(_)), "{error}");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(oauth.refresh_count(), 1);
    }

    #[tokio::test]
    async fn other_errors_are_not_retried() {
        let clock = FakeClock::new(1_000_000);
        let oauth = FakeOAuth::new(clock.clone(), 3600);
        let source = oauth.source(&account());

        let attempts = AtomicUsize::new(0);
        let error = with_fresh_token(&source, is_imap_rejection, |_token| {
            attempts.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(AppError::Imap("TCP connect failed: refused".into())) }
        })
        .await
        .unwrap_err();

        assert!(matches!(error, AppError::Imap(_)));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert_eq!(oauth.refresh_count(), 0);
    }
}
