//! Where the bearer token lives between launches.

use std::sync::{Arc, Mutex};

/// Keeps one bearer token. Errors are messages for the status line; a
/// failed save still leaves the user signed in for this run.
pub trait TokenStore: Send {
    fn load(&self) -> Option<String>;
    fn save(&self, token: &str) -> Result<(), String>;
    fn clear(&self);
}

/// The OS keyring: Keychain on macOS, Credential Manager on Windows, the
/// kernel keyutils keyring on Linux. One entry per server URL, so two
/// servers keep separate tokens.
pub struct KeyringStore {
    service: String,
    user: String,
}

impl KeyringStore {
    pub const SERVICE: &'static str = "email-rs";

    pub fn for_server(base_url: &str) -> Self {
        Self {
            service: Self::SERVICE.to_string(),
            user: base_url.trim_end_matches('/').to_string(),
        }
    }

    fn entry(&self) -> Result<keyring::Entry, String> {
        keyring::Entry::new(&self.service, &self.user).map_err(|err| err.to_string())
    }
}

impl TokenStore for KeyringStore {
    fn load(&self) -> Option<String> {
        self.entry()
            .ok()?
            .get_password()
            .ok()
            .filter(|token| !token.is_empty())
    }

    fn save(&self, token: &str) -> Result<(), String> {
        self.entry()?
            .set_password(token)
            .map_err(|err| format!("could not save the session in the keyring: {err}"))
    }

    fn clear(&self) {
        if let Ok(entry) = self.entry() {
            let _ = entry.delete_credential();
        }
    }
}

/// In-process store for tests and for `--no-keyring` runs. Clones share
/// the token.
#[derive(Clone, Default)]
pub struct MemoryStore(Arc<Mutex<Option<String>>>);

impl MemoryStore {
    pub fn with_token(token: &str) -> Self {
        Self(Arc::new(Mutex::new(Some(token.to_string()))))
    }

    pub fn token(&self) -> Option<String> {
        self.0.lock().ok().and_then(|token| token.clone())
    }
}

impl TokenStore for MemoryStore {
    fn load(&self) -> Option<String> {
        self.token()
    }

    fn save(&self, token: &str) -> Result<(), String> {
        if let Ok(mut slot) = self.0.lock() {
            *slot = Some(token.to_string());
        }
        Ok(())
    }

    fn clear(&self) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trips_and_clears() {
        let store = MemoryStore::default();
        assert_eq!(store.load(), None);
        store.save("t1").unwrap();
        assert_eq!(store.clone().load().as_deref(), Some("t1"));
        store.clear();
        assert_eq!(store.load(), None);
    }

    #[test]
    fn keyring_entry_is_keyed_by_server_without_trailing_slash() {
        let a = KeyringStore::for_server("http://localhost:8585/");
        let b = KeyringStore::for_server("http://localhost:8585");
        assert_eq!(a.user, b.user);
        assert_eq!(a.service, KeyringStore::SERVICE);
    }
}
