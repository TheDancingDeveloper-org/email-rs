use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use email_types::Message as MessageRow;
use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SmartFolderQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

pub async fn list_smart_messages(
    State(state): State<Arc<AppState>>,
    Path(kind): Path<String>,
    Query(q): Query<SmartFolderQuery>,
) -> Result<Json<Vec<MessageRow>>> {
    let per_page = q.per_page.unwrap_or(100).clamp(1, 500);
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;

    let rows = match kind.as_str() {
        "all" => {
            sqlx::query_as::<_, MessageRow>(
                r#"SELECT m.id, m.id AS occurrence_id, m.logical_id, m.account_id, m.folder_id, m.uid, m.message_id,
                          m.thread_id, m.subject, m.from_name, m.from_email, m.to_json,
                          m.date, m.is_read, m.is_flagged, m.is_draft, m.has_attachments, m.preview,
                          m.snoozed_until
                   FROM messages m
                   JOIN folders f ON m.folder_id = f.id
                   WHERE f.special_use = 'inbox'
                     AND (m.snoozed_until IS NULL OR m.snoozed_until <= datetime('now'))
                   ORDER BY m.date DESC
                   LIMIT ? OFFSET ?"#,
            )
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?
        }
        "unread" => {
            sqlx::query_as::<_, MessageRow>(
                r#"SELECT id, id AS occurrence_id, logical_id, account_id, folder_id, uid, message_id,
                          thread_id, subject, from_name, from_email, to_json,
                          date, is_read, is_flagged, is_draft, has_attachments, preview,
                          snoozed_until
                   FROM messages
                   WHERE is_read = 0
                     AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
                   ORDER BY date DESC
                   LIMIT ? OFFSET ?"#,
            )
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?
        }
        "flagged" => {
            sqlx::query_as::<_, MessageRow>(
                r#"SELECT id, id AS occurrence_id, logical_id, account_id, folder_id, uid, message_id,
                          thread_id, subject, from_name, from_email, to_json,
                          date, is_read, is_flagged, is_draft, has_attachments, preview,
                          snoozed_until
                   FROM messages
                   WHERE is_flagged = 1
                     AND (snoozed_until IS NULL OR snoozed_until <= datetime('now'))
                   ORDER BY date DESC
                   LIMIT ? OFFSET ?"#,
            )
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?
        }
        "snoozed" => {
            sqlx::query_as::<_, MessageRow>(
                r#"SELECT id, id AS occurrence_id, logical_id, account_id, folder_id, uid, message_id,
                          thread_id, subject, from_name, from_email, to_json,
                          date, is_read, is_flagged, is_draft, has_attachments, preview,
                          snoozed_until
                   FROM messages
                   WHERE snoozed_until IS NOT NULL AND snoozed_until > datetime('now')
                   ORDER BY snoozed_until ASC
                   LIMIT ? OFFSET ?"#,
            )
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?
        }
        other => {
            return Err(AppError::NotFound(format!(
                "unknown smart folder: {}",
                other
            )));
        }
    };

    Ok(Json(rows))
}
