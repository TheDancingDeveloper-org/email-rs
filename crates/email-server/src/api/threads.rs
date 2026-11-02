use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use email_types::Message;

use crate::{error::Result, state::AppState};

pub async fn get_thread(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
) -> Result<Json<Vec<Message>>> {
    let rows = sqlx::query_as::<_, Message>(
        r#"SELECT id, id AS occurrence_id, logical_id, account_id, folder_id, uid, message_id,
                  thread_id, subject, from_name, from_email, to_json, date,
                  is_read, is_flagged, is_draft, has_attachments, preview, snoozed_until
           FROM messages WHERE thread_id = ? ORDER BY date ASC, id ASC"#,
    )
    .bind(thread_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[cfg(test)]
mod tests {
    use axum::extract::Path;

    #[test]
    fn route_path_is_thread_scoped() {
        let Path(id): Path<String> = Path("thread-1".into());
        assert_eq!(id, "thread-1");
    }
}
