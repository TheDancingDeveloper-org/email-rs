//! Envelope encryption for provider credentials stored in SQLite.

use aes_gcm::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use sqlx::SqlitePool;

use crate::error::{AppError, Result};

/// Legacy envelope: `encv1:` + base64(nonce || ciphertext), no key id.
const LEGACY_PREFIX: &str = "encv1:";
/// Current envelope: `encv2:` + base64(key_id || nonce || ciphertext).
const PREFIX: &str = "encv2:";
const NONCE_LEN: usize = 12;
const DEFAULT_KEY_ID: u8 = 1;

#[derive(sqlx::FromRow)]
struct CredentialRow {
    id: String,
    oauth_token_json: Option<String>,
    password: Option<String>,
    smtp_password: Option<String>,
}

/// The configured credential keys: one current key that seals, plus any
/// previous keys that still open older envelopes during a rotation.
struct Keyring {
    current_id: u8,
    keys: Vec<(u8, [u8; 32])>,
}

impl Keyring {
    fn from_env() -> Result<Self> {
        #[cfg(test)]
        if std::env::var("EMAIL_RS_CREDENTIAL_KEY").is_err() {
            return Ok(Self {
                current_id: DEFAULT_KEY_ID,
                keys: vec![(DEFAULT_KEY_ID, [0x5a; 32])],
            });
        }

        let encoded = std::env::var("EMAIL_RS_CREDENTIAL_KEY").map_err(|_| {
            AppError::Auth(
                "EMAIL_RS_CREDENTIAL_KEY is required to store or use provider credentials"
                    .to_string(),
            )
        })?;
        let current_id = match std::env::var("EMAIL_RS_CREDENTIAL_KEY_ID") {
            Ok(id) => parse_key_id(&id, "EMAIL_RS_CREDENTIAL_KEY_ID")?,
            Err(_) => DEFAULT_KEY_ID,
        };
        let mut keys = vec![(current_id, decode_key(&encoded, "EMAIL_RS_CREDENTIAL_KEY")?)];
        if let Ok(previous) = std::env::var("EMAIL_RS_CREDENTIAL_PREVIOUS_KEYS") {
            for entry in previous.split(',').map(str::trim).filter(|e| !e.is_empty()) {
                let (id, key) = entry.split_once(':').ok_or_else(|| {
                    AppError::Auth(
                        "EMAIL_RS_CREDENTIAL_PREVIOUS_KEYS entries must be <key-id>:<base64-key>"
                            .to_string(),
                    )
                })?;
                let id = parse_key_id(id, "EMAIL_RS_CREDENTIAL_PREVIOUS_KEYS")?;
                if keys.iter().any(|(existing, _)| *existing == id) {
                    return Err(AppError::Auth(format!(
                        "credential key id {id} is configured more than once"
                    )));
                }
                keys.push((id, decode_key(key, "EMAIL_RS_CREDENTIAL_PREVIOUS_KEYS")?));
            }
        }
        Ok(Self { current_id, keys })
    }

    fn key(&self, id: u8) -> Option<&[u8; 32]> {
        self.keys.iter().find(|(k, _)| *k == id).map(|(_, key)| key)
    }

    fn seal(&self, plaintext: &str) -> Result<String> {
        let key = self
            .key(self.current_id)
            .ok_or_else(|| AppError::Auth("current credential key is missing".to_string()))?;
        let mut nonce = [0_u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher(key)?
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
            .map_err(|_| AppError::Auth("credential encryption failed".to_string()))?;
        let mut envelope = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
        envelope.push(self.current_id);
        envelope.extend_from_slice(&nonce);
        envelope.extend(ciphertext);
        Ok(format!("{PREFIX}{}", STANDARD.encode(envelope)))
    }

    fn open(&self, value: &str) -> Result<String> {
        if let Some(encoded) = value.strip_prefix(PREFIX) {
            let envelope = decode_envelope(encoded, 1)?;
            let key = self.key(envelope[0]).ok_or_else(|| {
                AppError::Auth(format!(
                    "stored provider credential uses key id {}, which is not configured",
                    envelope[0]
                ))
            })?;
            return decrypt(key, &envelope[1..]);
        }
        if let Some(encoded) = value.strip_prefix(LEGACY_PREFIX) {
            // Legacy envelopes carry no key id: try the current key, then the
            // previous ones.
            let envelope = decode_envelope(encoded, 0)?;
            let mut ordered: Vec<&[u8; 32]> = self.key(self.current_id).into_iter().collect();
            ordered.extend(
                self.keys
                    .iter()
                    .filter(|(id, _)| *id != self.current_id)
                    .map(|(_, key)| key),
            );
            for key in ordered {
                if let Ok(plaintext) = decrypt(key, &envelope) {
                    return Ok(plaintext);
                }
            }
            return Err(undecryptable());
        }
        Ok(value.to_string())
    }

    /// Returns a freshly sealed value when `value` is plaintext, a legacy
    /// envelope, or sealed with a key other than the current one.
    fn reseal(&self, value: &str) -> Result<Option<String>> {
        if let Some(encoded) = value.strip_prefix(PREFIX) {
            let envelope = decode_envelope(encoded, 1)?;
            if envelope[0] == self.current_id {
                // Verify it still opens so startup fails closed on a wrong key.
                self.open(value)?;
                return Ok(None);
            }
        }
        let plaintext = self.open(value)?;
        self.seal(&plaintext).map(Some)
    }
}

fn parse_key_id(raw: &str, var: &str) -> Result<u8> {
    raw.trim()
        .parse::<u8>()
        .map_err(|_| AppError::Auth(format!("{var} key ids must be integers from 0 to 255")))
}

fn decode_key(encoded: &str, var: &str) -> Result<[u8; 32]> {
    let decoded = STANDARD
        .decode(encoded.trim())
        .map_err(|_| AppError::Auth(format!("{var} must be base64-encoded")))?;
    decoded
        .try_into()
        .map_err(|_| AppError::Auth(format!("{var} must decode to exactly 32 bytes")))
}

fn cipher(key: &[u8; 32]) -> Result<Aes256Gcm> {
    Aes256Gcm::new_from_slice(key)
        .map_err(|_| AppError::Auth("credential cipher initialization failed".to_string()))
}

fn decode_envelope(encoded: &str, header_len: usize) -> Result<Vec<u8>> {
    let envelope = STANDARD
        .decode(encoded)
        .map_err(|_| AppError::Auth("stored credential envelope is invalid".to_string()))?;
    if envelope.len() <= header_len + NONCE_LEN {
        return Err(AppError::Auth(
            "stored credential envelope is truncated".to_string(),
        ));
    }
    Ok(envelope)
}

fn undecryptable() -> AppError {
    AppError::Auth(
        "stored provider credential cannot be decrypted with the configured key".to_string(),
    )
}

/// `envelope` is nonce || ciphertext.
fn decrypt(key: &[u8; 32], envelope: &[u8]) -> Result<String> {
    let plaintext = cipher(key)?
        .decrypt(
            Nonce::from_slice(&envelope[..NONCE_LEN]),
            &envelope[NONCE_LEN..],
        )
        .map_err(|_| undecryptable())?;
    String::from_utf8(plaintext)
        .map_err(|_| AppError::Auth("decrypted provider credential is not UTF-8".to_string()))
}

/// Encrypt `plaintext` with the current key. Always encrypts: input that
/// merely looks like an envelope is treated as plaintext.
pub fn seal(plaintext: &str) -> Result<String> {
    Keyring::from_env()?.seal(plaintext)
}

pub fn open(value: &str) -> Result<String> {
    Keyring::from_env()?.open(value)
}

/// Re-seal a stored value if it is plaintext, legacy, or sealed with a
/// non-current key. Returns `None` when it is already current.
pub fn reseal(value: &str) -> Result<Option<String>> {
    Keyring::from_env()?.reseal(value)
}

pub fn seal_optional(value: Option<&str>) -> Result<Option<String>> {
    value.map(seal).transpose()
}

pub fn open_optional(value: Option<String>) -> Result<Option<String>> {
    value.map(|value| open(&value)).transpose()
}

fn migrate_value(keyring: &Keyring, value: Option<String>) -> Result<(Option<String>, bool)> {
    match value {
        None => Ok((None, false)),
        Some(value) => match keyring.reseal(&value)? {
            Some(sealed) => Ok((Some(sealed), true)),
            None => Ok((Some(value), false)),
        },
    }
}

/// Encrypt legacy plaintext rows in place and re-seal envelopes written with
/// a legacy format or a previous key (the rotation step). If credentials exist,
/// startup fails closed when no valid key for them is configured.
pub async fn migrate_account_credentials(pool: &SqlitePool) -> anyhow::Result<()> {
    let rows: Vec<CredentialRow> =
        sqlx::query_as("SELECT id, oauth_token_json, password, smtp_password FROM accounts")
            .fetch_all(pool)
            .await?;
    let hooks: Vec<(String, String)> =
        sqlx::query_as("SELECT id, secret FROM webhooks WHERE secret IS NOT NULL")
            .fetch_all(pool)
            .await?;
    let has_credentials = !hooks.is_empty()
        || rows.iter().any(|row| {
            row.oauth_token_json.is_some() || row.password.is_some() || row.smtp_password.is_some()
        });
    if !has_credentials {
        return Ok(());
    }
    let keyring = Keyring::from_env().map_err(anyhow::Error::msg)?;
    migrate_with(pool, &keyring, rows, hooks).await
}

async fn migrate_with(
    pool: &SqlitePool,
    keyring: &Keyring,
    rows: Vec<CredentialRow>,
    hooks: Vec<(String, String)>,
) -> anyhow::Result<()> {
    for row in rows {
        let (oauth, a) =
            migrate_value(keyring, row.oauth_token_json).map_err(anyhow::Error::msg)?;
        let (password, b) = migrate_value(keyring, row.password).map_err(anyhow::Error::msg)?;
        let (smtp_password, c) =
            migrate_value(keyring, row.smtp_password).map_err(anyhow::Error::msg)?;
        if !(a || b || c) {
            continue;
        }
        sqlx::query(
            "UPDATE accounts SET oauth_token_json = ?, password = ?, smtp_password = ? WHERE id = ?",
        )
        .bind(oauth)
        .bind(password)
        .bind(smtp_password)
        .bind(row.id)
        .execute(pool)
        .await?;
    }
    for (id, secret) in hooks {
        if let Some(sealed) = keyring.reseal(&secret).map_err(anyhow::Error::msg)? {
            sqlx::query("UPDATE webhooks SET secret = ? WHERE id = ?")
                .bind(sealed)
                .bind(id)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_values_are_randomized_and_round_trip() {
        let first = seal("provider-secret").unwrap();
        let second = seal("provider-secret").unwrap();
        assert!(first.starts_with(PREFIX));
        assert_ne!(first, second);
        assert_eq!(open(&first).unwrap(), "provider-secret");
        assert!(!first.contains("provider-secret"));
    }

    fn test_keyring(current_id: u8, keys: &[(u8, u8)]) -> Keyring {
        Keyring {
            current_id,
            keys: keys.iter().map(|(id, fill)| (*id, [*fill; 32])).collect(),
        }
    }

    #[test]
    fn prefixed_plaintext_is_encrypted_not_passed_through() {
        // Regression for #61: seal() used to return anything starting with
        // the envelope prefix unchanged, storing it in plaintext.
        for input in ["encv1:not-really-sealed", "encv2:not-really-sealed"] {
            let sealed = seal(input).unwrap();
            assert_ne!(sealed, input);
            assert!(!sealed.contains("not-really-sealed"));
            assert_eq!(open(&sealed).unwrap(), input);
        }
    }

    #[test]
    fn envelope_carries_the_current_key_id() {
        let keyring = test_keyring(7, &[(7, 0x11)]);
        let sealed = keyring.seal("secret").unwrap();
        let envelope = STANDARD
            .decode(sealed.strip_prefix(PREFIX).unwrap())
            .unwrap();
        assert_eq!(envelope[0], 7);
        assert_eq!(keyring.open(&sealed).unwrap(), "secret");
    }

    #[test]
    fn previous_keys_open_and_reseal_with_the_current_key() {
        let old = test_keyring(1, &[(1, 0x11)]);
        let sealed = old.seal("secret").unwrap();

        let rotated = test_keyring(2, &[(2, 0x22), (1, 0x11)]);
        assert_eq!(rotated.open(&sealed).unwrap(), "secret");
        let resealed = rotated.reseal(&sealed).unwrap().unwrap();
        assert_eq!(rotated.reseal(&resealed).unwrap(), None);

        let new_only = test_keyring(2, &[(2, 0x22)]);
        assert_eq!(new_only.open(&resealed).unwrap(), "secret");
        assert!(new_only.open(&sealed).is_err());
    }

    #[test]
    fn legacy_v1_envelopes_open_and_are_upgraded() {
        let key = [0x11; 32];
        let nonce = [3_u8; NONCE_LEN];
        let mut envelope = nonce.to_vec();
        envelope.extend(
            cipher(&key)
                .unwrap()
                .encrypt(Nonce::from_slice(&nonce), b"legacy".as_ref())
                .unwrap(),
        );
        let v1 = format!("{LEGACY_PREFIX}{}", STANDARD.encode(envelope));

        let keyring = test_keyring(2, &[(2, 0x22), (1, 0x11)]);
        assert_eq!(keyring.open(&v1).unwrap(), "legacy");
        let upgraded = keyring.reseal(&v1).unwrap().unwrap();
        assert!(upgraded.starts_with(PREFIX));
        assert_eq!(keyring.open(&upgraded).unwrap(), "legacy");
    }

    #[test]
    fn current_envelope_with_wrong_key_fails_reseal() {
        let sealed = test_keyring(1, &[(1, 0x11)]).seal("secret").unwrap();
        assert!(test_keyring(1, &[(1, 0x99)]).reseal(&sealed).is_err());
    }

    #[test]
    fn plaintext_is_read_for_one_time_legacy_migration() {
        assert_eq!(open("legacy-secret").unwrap(), "legacy-secret");
    }

    #[tokio::test]
    async fn legacy_database_credentials_are_encrypted_in_place() {
        let _db_dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let path = _db_dir.path().join("email.db").display().to_string();
        let pool = crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap()
            .0;
        sqlx::query(
            r#"INSERT INTO accounts
                   (id, name, email, provider_type, auth_type, password, smtp_password)
               VALUES ('account-1', 'Test', 'test@example.com', 'generic_imap',
                       'basic', 'imap-secret', 'smtp-secret')"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        migrate_account_credentials(&pool).await.unwrap();
        let stored: (String, String) =
            sqlx::query_as("SELECT password, smtp_password FROM accounts WHERE id = 'account-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(stored.0.starts_with(PREFIX));
        assert!(stored.1.starts_with(PREFIX));
        assert_eq!(open(&stored.0).unwrap(), "imap-secret");
        assert_eq!(open(&stored.1).unwrap(), "smtp-secret");
    }

    #[tokio::test]
    async fn startup_migration_rotates_rows_to_the_current_key() {
        let _db_dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let path = _db_dir.path().join("email.db").display().to_string();
        let pool = crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap()
            .0;
        let old = test_keyring(1, &[(1, 0x11)]);
        sqlx::query(
            r#"INSERT INTO accounts
                   (id, name, email, provider_type, auth_type, password)
               VALUES ('account-1', 'Test', 'test@example.com', 'generic_imap', 'basic', ?)"#,
        )
        .bind(old.seal("imap-secret").unwrap())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO webhooks (id, url, secret, events) VALUES ('hook-1', 'https://example.test', ?, 'all')",
        )
        .bind(old.seal("hook-secret").unwrap())
        .execute(&pool)
        .await
        .unwrap();

        let rotated = test_keyring(2, &[(2, 0x22), (1, 0x11)]);
        let rows =
            sqlx::query_as("SELECT id, oauth_token_json, password, smtp_password FROM accounts")
                .fetch_all(&pool)
                .await
                .unwrap();
        let hooks = sqlx::query_as("SELECT id, secret FROM webhooks WHERE secret IS NOT NULL")
            .fetch_all(&pool)
            .await
            .unwrap();
        migrate_with(&pool, &rotated, rows, hooks).await.unwrap();

        let new_only = test_keyring(2, &[(2, 0x22)]);
        let password: String =
            sqlx::query_scalar("SELECT password FROM accounts WHERE id = 'account-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let secret: String = sqlx::query_scalar("SELECT secret FROM webhooks WHERE id = 'hook-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(new_only.open(&password).unwrap(), "imap-secret");
        assert_eq!(new_only.open(&secret).unwrap(), "hook-secret");
    }
}
