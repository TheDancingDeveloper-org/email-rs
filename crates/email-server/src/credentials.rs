//! Envelope encryption for provider credentials stored in SQLite.

use aes_gcm::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use sqlx::SqlitePool;

use crate::error::{AppError, Result};

const PREFIX: &str = "encv1:";
const NONCE_LEN: usize = 12;

#[derive(sqlx::FromRow)]
struct CredentialRow {
    id: String,
    oauth_token_json: Option<String>,
    password: Option<String>,
    smtp_password: Option<String>,
}

fn key_bytes() -> Result<[u8; 32]> {
    #[cfg(test)]
    if std::env::var("EMAIL_RS_CREDENTIAL_KEY").is_err() {
        return Ok([0x5a; 32]);
    }

    let encoded = std::env::var("EMAIL_RS_CREDENTIAL_KEY").map_err(|_| {
        AppError::Auth(
            "EMAIL_RS_CREDENTIAL_KEY is required to store or use provider credentials".to_string(),
        )
    })?;
    let decoded = STANDARD.decode(encoded.trim()).map_err(|_| {
        AppError::Auth("EMAIL_RS_CREDENTIAL_KEY must be base64-encoded".to_string())
    })?;
    decoded.try_into().map_err(|_| {
        AppError::Auth("EMAIL_RS_CREDENTIAL_KEY must decode to exactly 32 bytes".to_string())
    })
}

pub fn seal(plaintext: &str) -> Result<String> {
    if plaintext.starts_with(PREFIX) {
        return Ok(plaintext.to_string());
    }
    let cipher = Aes256Gcm::new_from_slice(&key_bytes()?)
        .map_err(|_| AppError::Auth("credential cipher initialization failed".to_string()))?;
    let mut nonce = [0_u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
        .map_err(|_| AppError::Auth("credential encryption failed".to_string()))?;
    let mut envelope = nonce.to_vec();
    envelope.extend(ciphertext);
    Ok(format!("{PREFIX}{}", STANDARD.encode(envelope)))
}

pub fn open(value: &str) -> Result<String> {
    let Some(encoded) = value.strip_prefix(PREFIX) else {
        return Ok(value.to_string());
    };
    let envelope = STANDARD
        .decode(encoded)
        .map_err(|_| AppError::Auth("stored credential envelope is invalid".to_string()))?;
    if envelope.len() <= NONCE_LEN {
        return Err(AppError::Auth(
            "stored credential envelope is truncated".to_string(),
        ));
    }
    let cipher = Aes256Gcm::new_from_slice(&key_bytes()?)
        .map_err(|_| AppError::Auth("credential cipher initialization failed".to_string()))?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&envelope[..NONCE_LEN]),
            &envelope[NONCE_LEN..],
        )
        .map_err(|_| {
            AppError::Auth(
                "stored provider credential cannot be decrypted with the configured key"
                    .to_string(),
            )
        })?;
    String::from_utf8(plaintext)
        .map_err(|_| AppError::Auth("decrypted provider credential is not UTF-8".to_string()))
}

pub fn seal_optional(value: Option<&str>) -> Result<Option<String>> {
    value.map(seal).transpose()
}

pub fn open_optional(value: Option<String>) -> Result<Option<String>> {
    value.map(|value| open(&value)).transpose()
}

fn migrate_value(value: Option<String>) -> Result<Option<String>> {
    value
        .map(|value| {
            if value.starts_with(PREFIX) {
                open(&value)?;
                Ok(value)
            } else {
                seal(&value)
            }
        })
        .transpose()
}

/// Encrypt legacy plaintext rows in place. If credentials exist, startup fails
/// closed when no valid master key is configured.
pub async fn migrate_account_credentials(pool: &SqlitePool) -> anyhow::Result<()> {
    let rows: Vec<CredentialRow> =
        sqlx::query_as("SELECT id, oauth_token_json, password, smtp_password FROM accounts")
            .fetch_all(pool)
            .await?;
    for row in rows {
        if row.oauth_token_json.is_none() && row.password.is_none() && row.smtp_password.is_none() {
            continue;
        }
        let oauth = migrate_value(row.oauth_token_json).map_err(anyhow::Error::msg)?;
        let password = migrate_value(row.password).map_err(anyhow::Error::msg)?;
        let smtp_password = migrate_value(row.smtp_password).map_err(anyhow::Error::msg)?;
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
}
