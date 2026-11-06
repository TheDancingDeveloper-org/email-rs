use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use email_types::{CreateIdentity, Identity, UpdateIdentity, DEFAULT_IDENTITY_ID};
use lettre::{message::Mailbox, Address};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::state::AppState;

const MAX_NAME_CHARS: usize = 200;

pub async fn list_identities(
    State(state): State<Arc<AppState>>,
    Path(account_id): Path<String>,
) -> Result<Json<Vec<Identity>>> {
    Ok(Json(list(&state.pool, &account_id).await?))
}

pub async fn create_identity(
    State(state): State<Arc<AppState>>,
    Path(account_id): Path<String>,
    Json(body): Json<CreateIdentity>,
) -> Result<Json<Identity>> {
    Ok(Json(create(&state.pool, &account_id, body).await?))
}

pub async fn delete_identity(
    State(state): State<Arc<AppState>>,
    Path((account_id, identity_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    if identity_id == DEFAULT_IDENTITY_ID {
        return Err(AppError::Validation(
            "the default identity cannot be deleted".into(),
        ));
    }
    let rows = sqlx::query("DELETE FROM identities WHERE id = ? AND account_id = ?")
        .bind(&identity_id)
        .bind(&account_id)
        .execute(&state.pool)
        .await?;
    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "identity {identity_id} not found"
        )));
    }
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

pub async fn update_identity(
    State(state): State<Arc<AppState>>,
    Path((account_id, identity_id)): Path<(String, String)>,
    Json(body): Json<UpdateIdentity>,
) -> Result<Json<Identity>> {
    Ok(Json(
        update(&state.pool, account_id, identity_id, body).await?,
    ))
}

pub(crate) async fn update(
    pool: &SqlitePool,
    account_id: String,
    identity_id: String,
    body: UpdateIdentity,
) -> Result<Identity> {
    if identity_id == DEFAULT_IDENTITY_ID {
        return Err(AppError::Validation(
            "the default identity cannot be updated here".into(),
        ));
    }
    let current = sqlx::query_as::<_, Identity>(
        "SELECT id, account_id, name, email, 0 AS is_default, signature, signature_html, signature_placement FROM identities WHERE id = ? AND account_id = ?",
    )
    .bind(&identity_id).bind(&account_id).fetch_optional(pool).await?
    .ok_or_else(|| AppError::NotFound(format!("identity {identity_id} not found")))?;
    let (name, email) = validate_name_and_email(
        pool,
        &account_id,
        Some(&identity_id),
        body.name.as_deref().unwrap_or(&current.name),
        body.email.as_deref().unwrap_or(&current.email),
    )
    .await?;
    let signature = body.signature.unwrap_or(current.signature);
    let signature_html = body.signature_html.unwrap_or(current.signature_html);
    let placement = body
        .signature_placement
        .unwrap_or(current.signature_placement);
    validate_signature_placement(&placement)?;
    sqlx::query("UPDATE identities SET name = ?, email = ?, signature = ?, signature_html = ?, signature_placement = ? WHERE id = ? AND account_id = ?")
        .bind(&name).bind(&email).bind(&signature).bind(&signature_html).bind(&placement).bind(&identity_id).bind(&account_id).execute(pool).await?;
    Ok(Identity {
        id: identity_id,
        account_id,
        name,
        email,
        is_default: false,
        signature,
        signature_html,
        signature_placement: placement,
    })
}

/// The account's own identity first, then its aliases in the order added.
pub(crate) async fn list(pool: &SqlitePool, account_id: &str) -> Result<Vec<Identity>> {
    let mut identities = vec![default_identity(pool, account_id).await?];
    identities.extend(
        sqlx::query_as::<_, Identity>(
            "SELECT id, account_id, name, email, 0 AS is_default, signature, signature_html, signature_placement FROM identities WHERE account_id = ? ORDER BY created_at, rowid",
        )
        .bind(account_id)
        .fetch_all(pool)
        .await?,
    );
    Ok(identities)
}

async fn default_identity(pool: &SqlitePool, account_id: &str) -> Result<Identity> {
    sqlx::query_as::<_, Identity>(
        "SELECT ? AS id, id AS account_id, name, email, 1 AS is_default, signature, NULL AS signature_html, 'below' AS signature_placement FROM accounts WHERE id = ?",
    )
    .bind(DEFAULT_IDENTITY_ID)
    .bind(account_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("account {account_id} not found")))
}

pub(crate) async fn create(
    pool: &SqlitePool,
    account_id: &str,
    body: CreateIdentity,
) -> Result<Identity> {
    validate_signature_placement(&body.signature_placement)?;
    let (name, email) =
        validate_name_and_email(pool, account_id, None, &body.name, &body.email).await?;
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO identities (id, account_id, name, email, signature, signature_html, signature_placement) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(&id).bind(account_id).bind(&name).bind(&email)
        .bind(&body.signature).bind(&body.signature_html).bind(&body.signature_placement)
        .execute(pool)
        .await?;
    Ok(Identity {
        id,
        account_id: account_id.to_string(),
        name,
        email,
        is_default: false,
        signature: body.signature,
        signature_html: body.signature_html,
        signature_placement: body.signature_placement,
    })
}

/// The `From` header value for `identity_id` on `account_id`. `None` or
/// `"default"` is the account's own address; any other id must be one of that
/// account's identities.
pub(crate) async fn resolve_from(
    pool: &SqlitePool,
    account_id: &str,
    identity_id: Option<&str>,
) -> Result<String> {
    let identity = match identity_id {
        None | Some(DEFAULT_IDENTITY_ID) => default_identity(pool, account_id).await?,
        Some(id) => sqlx::query_as::<_, Identity>(
            "SELECT id, account_id, name, email, 0 AS is_default, signature, signature_html, signature_placement FROM identities WHERE id = ? AND account_id = ?",
        )
        .bind(id)
        .bind(account_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::Validation(format!("identity {id} not found for this account")))?,
    };
    let address = identity
        .email
        .parse::<Address>()
        .map_err(|_| AppError::Validation("identity email is not a valid address".into()))?;
    let name = identity.name.trim();
    Ok(Mailbox::new((!name.is_empty()).then(|| name.to_string()), address).to_string())
}

/// Trimmed, header-safe name and address for an identity on `account_id`.
/// The address must parse and must not already belong to the account or to
/// another of its identities; `own_id` is the identity being updated, whose
/// own row does not count as a clash.
async fn validate_name_and_email(
    pool: &SqlitePool,
    account_id: &str,
    own_id: Option<&str>,
    name: &str,
    email: &str,
) -> Result<(String, String)> {
    let primary = default_identity(pool, account_id).await?;
    let name = name.trim().to_string();
    if name.chars().count() > MAX_NAME_CHARS || name.chars().any(char::is_control) {
        return Err(AppError::Validation("identity name is not valid".into()));
    }
    let email = email.trim().to_string();
    email
        .parse::<Address>()
        .map_err(|_| AppError::Validation("identity email is not a valid address".into()))?;
    let taken = primary.email.eq_ignore_ascii_case(&email)
        || sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM identities WHERE account_id = ? AND lower(email) = lower(?) AND id IS NOT ?",
        )
        .bind(account_id)
        .bind(&email)
        .bind(own_id)
        .fetch_one(pool)
        .await?
            > 0;
    if taken {
        return Err(AppError::Conflict(
            "this account already has an identity with that address".into(),
        ));
    }
    Ok((name, email))
}

fn validate_signature_placement(value: &str) -> Result<()> {
    if matches!(value, "above" | "below") {
        Ok(())
    } else {
        Err(AppError::Validation(
            "signaturePlacement must be 'above' or 'below'".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool_with_account() -> SqlitePool {
        let pool = crate::db::create_pool("sqlite::memory:").await.unwrap().0;
        for (id, email) in [("a1", "me@example.com"), ("a2", "other@example.com")] {
            sqlx::query("INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES (?, 'Me', ?, 'imap', 'basic')")
                .bind(id)
                .bind(email)
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
    }

    fn new_identity(name: &str, email: &str) -> CreateIdentity {
        CreateIdentity {
            name: name.into(),
            email: email.into(),
            signature: None,
            signature_html: None,
            signature_placement: "below".into(),
        }
    }

    #[tokio::test]
    async fn list_puts_the_account_address_first_then_aliases_per_account() {
        let pool = pool_with_account().await;
        let alias = create(&pool, "a1", new_identity("Sales", "sales@example.com"))
            .await
            .unwrap();
        create(&pool, "a2", new_identity("Other", "x@example.com"))
            .await
            .unwrap();

        let listed = list(&pool, "a1").await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, DEFAULT_IDENTITY_ID);
        assert!(listed[0].is_default);
        assert_eq!(listed[0].email, "me@example.com");
        assert_eq!(listed[1], alias);
        assert!(!listed[1].is_default);
        assert!(matches!(
            list(&pool, "missing").await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn create_rejects_bad_or_duplicate_addresses() {
        let pool = pool_with_account().await;
        for body in [
            new_identity("A", "not-an-address"),
            new_identity("Bad\nName", "a@example.com"),
        ] {
            assert!(matches!(
                create(&pool, "a1", body).await,
                Err(AppError::Validation(_))
            ));
        }
        create(&pool, "a1", new_identity("A", "alias@example.com"))
            .await
            .unwrap();
        for email in ["ALIAS@example.com", "Me@Example.com"] {
            assert!(matches!(
                create(&pool, "a1", new_identity("B", email)).await,
                Err(AppError::Conflict(_))
            ));
        }
        assert!(matches!(
            create(&pool, "missing", new_identity("B", "b@example.com")).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn resolve_from_uses_the_chosen_identity_and_only_for_its_account() {
        let pool = pool_with_account().await;
        let alias = create(
            &pool,
            "a1",
            new_identity("Sales, Team", "sales@example.com"),
        )
        .await
        .unwrap();

        assert_eq!(
            resolve_from(&pool, "a1", None).await.unwrap(),
            "Me <me@example.com>"
        );
        assert_eq!(
            resolve_from(&pool, "a1", Some(DEFAULT_IDENTITY_ID))
                .await
                .unwrap(),
            "Me <me@example.com>"
        );
        let from = resolve_from(&pool, "a1", Some(&alias.id)).await.unwrap();
        let parsed: Mailbox = from.parse().unwrap();
        assert_eq!(parsed.email.to_string(), "sales@example.com");
        assert_eq!(parsed.name.as_deref(), Some("Sales, Team"));
        // Another account's identity id, and unknown ids, are refused.
        assert!(matches!(
            resolve_from(&pool, "a2", Some(&alias.id)).await,
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            resolve_from(&pool, "a1", Some("nope")).await,
            Err(AppError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn deleting_an_account_removes_its_identities() {
        let pool = pool_with_account().await;
        create(&pool, "a1", new_identity("A", "alias@example.com"))
            .await
            .unwrap();
        sqlx::query("DELETE FROM accounts WHERE id = 'a1'")
            .execute(&pool)
            .await
            .unwrap();
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM identities")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 0);
    }

    fn patch(json: serde_json::Value) -> UpdateIdentity {
        serde_json::from_value(json).unwrap()
    }

    #[tokio::test]
    async fn update_rejects_header_injection_and_duplicate_addresses() {
        let pool = pool_with_account().await;
        let alias = create(&pool, "a1", new_identity("A", "alias@example.com"))
            .await
            .unwrap();
        create(&pool, "a1", new_identity("B", "b@example.com"))
            .await
            .unwrap();
        for body in [
            serde_json::json!({"name": "Evil\r\nBcc: x@evil.test"}),
            serde_json::json!({"name": "x".repeat(MAX_NAME_CHARS + 1)}),
            serde_json::json!({"email": "not-an-address"}),
        ] {
            assert!(matches!(
                update(&pool, "a1".into(), alias.id.clone(), patch(body)).await,
                Err(AppError::Validation(_))
            ));
        }
        for email in ["B@example.com", "me@example.com"] {
            assert!(matches!(
                update(
                    &pool,
                    "a1".into(),
                    alias.id.clone(),
                    patch(serde_json::json!({ "email": email }))
                )
                .await,
                Err(AppError::Conflict(_))
            ));
        }
        // Its own address is not a clash.
        update(
            &pool,
            "a1".into(),
            alias.id.clone(),
            patch(serde_json::json!({"email": "ALIAS@example.com"})),
        )
        .await
        .unwrap();
        let stored: String = sqlx::query_scalar("SELECT name FROM identities WHERE id = ?")
            .bind(&alias.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored, "A");
    }

    #[tokio::test]
    async fn update_changes_fields_and_clears_a_signature_with_null() {
        let pool = pool_with_account().await;
        let mut body = new_identity("Sales", "sales@example.com");
        body.signature = Some("Regards".into());
        body.signature_html = Some("<p>Regards</p>".into());
        let alias = create(&pool, "a1", body).await.unwrap();

        let updated = update(
            &pool,
            "a1".into(),
            alias.id.clone(),
            patch(serde_json::json!({
                "name": "  Sales Team  ",
                "signature": null,
                "signaturePlacement": "above"
            })),
        )
        .await
        .unwrap();
        assert_eq!(updated.name, "Sales Team");
        assert_eq!(updated.email, "sales@example.com");
        assert_eq!(updated.signature, None);
        assert_eq!(updated.signature_html.as_deref(), Some("<p>Regards</p>"));
        assert_eq!(updated.signature_placement, "above");
        let listed = list(&pool, "a1").await.unwrap();
        assert_eq!(listed[1], updated);
    }
}
