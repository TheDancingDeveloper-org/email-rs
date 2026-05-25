use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use email_types::Message as MessageRow;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub account_id: Option<String>,
}

// ── FTS5 query helpers ────────────────────────────────────────────────────────

/// Wrap each whitespace-separated word in double quotes for FTS5 exact matching.
/// Double-quotes inside words are escaped by doubling.
fn fts_query(term: &str) -> String {
    term.split_whitespace()
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Like `fts_query` but the last word gets a `*` prefix suffix for autocomplete.
fn fts_prefix_query(term: &str) -> String {
    let words: Vec<&str> = term.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = words[..words.len() - 1]
        .iter()
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect();
    let last = words.last().unwrap();
    parts.push(format!("\"{}\"*", last.replace('"', "\"\"")));
    parts.join(" ")
}

// ── Full search ───────────────────────────────────────────────────────────────

pub async fn search_messages(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<SearchResponse>> {
    let term = params.q.trim();
    if term.is_empty() {
        return Ok(Json(SearchResponse {
            messages: vec![],
            events: vec![],
        }));
    }

    let messages = if state.has_fts {
        search_fts(&state, term, params.account_id.as_deref()).await?
    } else {
        search_like(&state, term, params.account_id.as_deref()).await?
    };
    let events = search_calendar_events(&state, term, params.account_id.as_deref(), 20).await?;

    Ok(Json(SearchResponse { messages, events }))
}

async fn search_fts(
    state: &AppState,
    term: &str,
    account_id: Option<&str>,
) -> Result<Vec<MessageRow>> {
    let q = fts_query(term);

    // Column weights: message_id(unindexed)=0, subject=10, from_name=5, from_email=5, preview=2
    let rows = if let Some(aid) = account_id {
        sqlx::query_as::<_, MessageRow>(
            r#"SELECT m.id, m.account_id, m.folder_id, m.uid, m.message_id,
                      m.thread_id, m.subject, m.from_name, m.from_email, m.to_json,
                      m.date, m.is_read, m.is_flagged, m.is_draft, m.has_attachments, m.preview,
                      m.snoozed_until
               FROM messages_fts fts
               JOIN messages m ON m.id = fts.message_id
               WHERE messages_fts MATCH ? AND m.account_id = ?
               ORDER BY bm25(messages_fts, 0, 10, 5, 5, 2)
               LIMIT 50"#,
        )
        .bind(&q)
        .bind(aid)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, MessageRow>(
            r#"SELECT m.id, m.account_id, m.folder_id, m.uid, m.message_id,
                      m.thread_id, m.subject, m.from_name, m.from_email, m.to_json,
                      m.date, m.is_read, m.is_flagged, m.is_draft, m.has_attachments, m.preview,
                      m.snoozed_until
               FROM messages_fts fts
               JOIN messages m ON m.id = fts.message_id
               WHERE messages_fts MATCH ?
               ORDER BY bm25(messages_fts, 0, 10, 5, 5, 2)
               LIMIT 50"#,
        )
        .bind(&q)
        .fetch_all(&state.pool)
        .await?
    };

    Ok(rows)
}

async fn search_like(
    state: &AppState,
    term: &str,
    account_id: Option<&str>,
) -> Result<Vec<MessageRow>> {
    let q_sub = format!("%{term}%");
    let q_word_start = format!("{term} %");
    let q_word_mid = format!("% {term}%");
    let score_expr = r#"CASE
        WHEN lower(m.subject) LIKE lower(?) OR lower(m.subject) LIKE lower(?) THEN 4
        WHEN m.subject LIKE ? THEN 3
        WHEN m.from_name LIKE ? OR m.from_email LIKE ? THEN 2
        WHEN m.preview LIKE ? THEN 1
        ELSE 0 END"#;

    let rows = if let Some(aid) = account_id {
        sqlx::query_as::<_, MessageRow>(&format!(
            r#"SELECT m.id, m.account_id, m.folder_id, m.uid, m.message_id,
                      m.thread_id, m.subject, m.from_name, m.from_email, m.to_json,
                      m.date, m.is_read, m.is_flagged, m.is_draft, m.has_attachments, m.preview,
                      m.snoozed_until
               FROM messages m
               LEFT JOIN message_bodies mb ON mb.message_id = m.id
               WHERE m.account_id = ?
                 AND (m.subject LIKE ? OR m.from_name LIKE ? OR m.from_email LIKE ?
                      OR m.preview LIKE ? OR mb.text_body LIKE ?)
               GROUP BY m.id
               ORDER BY {score_expr} DESC, m.date DESC
               LIMIT 50"#,
        ))
        .bind(aid)
        .bind(&q_sub)
        .bind(&q_sub)
        .bind(&q_sub)
        .bind(&q_sub)
        .bind(&q_sub)
        .bind(&q_word_start)
        .bind(&q_word_mid)
        .bind(&q_sub)
        .bind(&q_sub)
        .bind(&q_sub)
        .bind(&q_sub)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, MessageRow>(&format!(
            r#"SELECT m.id, m.account_id, m.folder_id, m.uid, m.message_id,
                      m.thread_id, m.subject, m.from_name, m.from_email, m.to_json,
                      m.date, m.is_read, m.is_flagged, m.is_draft, m.has_attachments, m.preview,
                      m.snoozed_until
               FROM messages m
               LEFT JOIN message_bodies mb ON mb.message_id = m.id
               WHERE m.subject LIKE ? OR m.from_name LIKE ? OR m.from_email LIKE ?
                  OR m.preview LIKE ? OR mb.text_body LIKE ?
               GROUP BY m.id
               ORDER BY {score_expr} DESC, m.date DESC
               LIMIT 50"#,
        ))
        .bind(&q_sub)
        .bind(&q_sub)
        .bind(&q_sub)
        .bind(&q_sub)
        .bind(&q_sub)
        .bind(&q_word_start)
        .bind(&q_word_mid)
        .bind(&q_sub)
        .bind(&q_sub)
        .bind(&q_sub)
        .bind(&q_sub)
        .fetch_all(&state.pool)
        .await?
    };

    Ok(rows)
}

// ── Search + suggest response types ──────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSearchRow {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub start_at: String,
    pub end_at: String,
    pub location: Option<String>,
    pub is_all_day: bool,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub messages: Vec<MessageRow>,
    pub events: Vec<CalendarSearchRow>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSuggestRow {
    pub id: String,
    pub title: String,
    pub start_at: String,
}

#[derive(Debug, Serialize)]
pub struct SuggestResponse {
    pub messages: Vec<SuggestRow>,
    pub events: Vec<CalendarSuggestRow>,
}

// ── Suggest (autocomplete) ────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SuggestRow {
    pub id: String,
    pub folder_id: String,
    pub subject: Option<String>,
    pub from_name: Option<String>,
    pub from_email: Option<String>,
    pub date: Option<String>,
}

pub async fn suggest_messages(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<SuggestResponse>> {
    let term = params.q.trim();
    if term.len() < 2 {
        return Ok(Json(SuggestResponse {
            messages: vec![],
            events: vec![],
        }));
    }

    let messages = if state.has_fts {
        let q = fts_prefix_query(term);
        if let Some(aid) = &params.account_id {
            sqlx::query_as::<_, SuggestRow>(
                r#"SELECT m.id, m.folder_id, m.subject, m.from_name, m.from_email, m.date
                   FROM messages_fts fts
                   JOIN messages m ON m.id = fts.message_id
                   WHERE messages_fts MATCH ? AND m.account_id = ?
                   ORDER BY bm25(messages_fts, 0, 10, 5, 5, 2)
                   LIMIT 8"#,
            )
            .bind(&q)
            .bind(aid)
            .fetch_all(&state.pool)
            .await?
        } else {
            sqlx::query_as::<_, SuggestRow>(
                r#"SELECT m.id, m.folder_id, m.subject, m.from_name, m.from_email, m.date
                   FROM messages_fts fts
                   JOIN messages m ON m.id = fts.message_id
                   WHERE messages_fts MATCH ?
                   ORDER BY bm25(messages_fts, 0, 10, 5, 5, 2)
                   LIMIT 8"#,
            )
            .bind(&q)
            .fetch_all(&state.pool)
            .await?
        }
    } else {
        // Prefix LIKE on subject/from only — no leading wildcard so index is usable
        let q_prefix = format!("{term}%");
        let q_sub = format!("%{term}%");
        sqlx::query_as::<_, SuggestRow>(
            r#"SELECT id, folder_id, subject, from_name, from_email, date
               FROM messages
               WHERE subject LIKE ? OR from_name LIKE ? OR from_email LIKE ?
               ORDER BY date DESC
               LIMIT 8"#,
        )
        .bind(&q_prefix)
        .bind(&q_sub)
        .bind(&q_sub)
        .fetch_all(&state.pool)
        .await?
    };
    let events = suggest_calendar_events(&state, term).await?;

    Ok(Json(SuggestResponse { messages, events }))
}

async fn suggest_calendar_events(state: &AppState, term: &str) -> Result<Vec<CalendarSuggestRow>> {
    let q = format!("%{term}%");
    sqlx::query_as::<_, CalendarSuggestRow>(
        "SELECT id, title, start_at FROM calendar_events \
         WHERE title LIKE ? OR description LIKE ? \
         ORDER BY start_at \
         LIMIT 3",
    )
    .bind(&q)
    .bind(&q)
    .fetch_all(&state.pool)
    .await
    .map_err(Into::into)
}

async fn search_calendar_events(
    state: &AppState,
    term: &str,
    account_id: Option<&str>,
    limit: i64,
) -> Result<Vec<CalendarSearchRow>> {
    let q = format!("%{term}%");
    if let Some(aid) = account_id {
        sqlx::query_as::<_, CalendarSearchRow>(
            "SELECT id, title, description, start_at, end_at, location, is_all_day \
             FROM calendar_events \
             WHERE account_id = ? AND (title LIKE ? OR description LIKE ?) \
             ORDER BY start_at \
             LIMIT ?",
        )
        .bind(aid)
        .bind(&q)
        .bind(&q)
        .bind(limit)
        .fetch_all(&state.pool)
        .await
        .map_err(Into::into)
    } else {
        sqlx::query_as::<_, CalendarSearchRow>(
            "SELECT id, title, description, start_at, end_at, location, is_all_day \
             FROM calendar_events \
             WHERE title LIKE ? OR description LIKE ? \
             ORDER BY start_at \
             LIMIT ?",
        )
        .bind(&q)
        .bind(&q)
        .bind(limit)
        .fetch_all(&state.pool)
        .await
        .map_err(Into::into)
    }
}

// ── Contact suggest ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ContactRow {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct ContactQuery {
    pub q: String,
}

pub async fn suggest_contacts(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ContactQuery>,
) -> Result<Json<Vec<ContactRow>>> {
    let term = params.q.trim();
    if term.len() < 2 {
        return Ok(Json(vec![]));
    }
    let q = format!("%{term}%");
    let rows = sqlx::query_as::<_, ContactRow>(
        r#"SELECT from_name AS name, from_email AS email
           FROM messages
           WHERE from_email IS NOT NULL
             AND (from_email LIKE ? OR from_name LIKE ?)
           GROUP BY from_email
           ORDER BY MAX(date) DESC
           LIMIT 10"#,
    )
    .bind(&q)
    .bind(&q)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[cfg(test)]
mod tests {
    use super::{fts_prefix_query, fts_query};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use std::sync::Arc;
    use tower::ServiceExt;

    // ── fts_query unit tests ─────────────────────────────────────────────────────

    #[test]
    fn fts_query_single_word() {
        assert_eq!(fts_query("hello"), "\"hello\"");
    }

    #[test]
    fn fts_query_multiple_words() {
        assert_eq!(fts_query("hello world"), "\"hello\" \"world\"");
    }

    #[test]
    fn fts_query_empty_input() {
        assert_eq!(fts_query(""), "");
    }

    #[test]
    fn fts_query_collapses_whitespace() {
        assert_eq!(fts_query("  hello   world  "), "\"hello\" \"world\"");
    }

    #[test]
    fn fts_query_escapes_embedded_double_quotes() {
        assert_eq!(fts_query("hel\"lo"), "\"hel\"\"lo\"");
    }

    #[test]
    fn fts_prefix_query_single_word_appends_star() {
        assert_eq!(fts_prefix_query("hell"), "\"hell\"*");
    }

    #[test]
    fn fts_prefix_query_multi_word_star_on_last_only() {
        assert_eq!(fts_prefix_query("hello wor"), "\"hello\" \"wor\"*");
    }

    #[test]
    fn fts_prefix_query_empty_returns_empty() {
        assert_eq!(fts_prefix_query(""), "");
    }

    #[test]
    fn fts_prefix_query_three_words() {
        assert_eq!(fts_prefix_query("a b c"), "\"a\" \"b\" \"c\"*");
    }

    #[test]
    fn fts_prefix_query_escapes_quotes_in_last_word() {
        assert_eq!(fts_prefix_query("he\"l"), "\"he\"\"l\"*");
    }

    // ── Integration tests (in-memory SQLite) ────────────────────────────────────

    async fn make_state(with_fts: bool) -> Arc<crate::state::AppState> {
        let path = format!("/tmp/email_test_{}.db", uuid::Uuid::new_v4());
        let (pool, has_fts) = crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap();
        Arc::new(crate::state::AppState::new(pool, has_fts && with_fts))
    }

    async fn seed(
        pool: &sqlx::SqlitePool,
        id: &str,
        account_id: &str,
        folder_id: &str,
        subject: &str,
        from_email: &str,
        from_name: &str,
        preview: &str,
    ) {
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO accounts (id, name, email, provider_type, auth_type) VALUES (?,?,?,?,?)",
        )
        .bind(account_id)
        .bind("Test")
        .bind(format!("{account_id}@test.com"))
        .bind("generic_imap")
        .bind("password")
        .execute(pool)
        .await;
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO folders (id, account_id, name, full_path) VALUES (?,?,?,?)",
        )
        .bind(folder_id)
        .bind(account_id)
        .bind("INBOX")
        .bind("INBOX")
        .execute(pool)
        .await;
        sqlx::query(
            "INSERT INTO messages (id, account_id, folder_id, uid, subject, from_email, from_name, preview, message_id) VALUES (?,?,?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind(account_id)
        .bind(folder_id)
        .bind(1i64)
        .bind(subject)
        .bind(from_email)
        .bind(from_name)
        .bind(preview)
        .bind(format!("<{id}>"))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn do_get(router: axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let req = Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn search_empty_query_returns_empty() {
        let state = make_state(true).await;
        let (status, body) = do_get(crate::api::router(state), "/search?q=").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["messages"], serde_json::json!([]));
        assert_eq!(body["events"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn search_whitespace_only_returns_empty() {
        let state = make_state(true).await;
        let (status, body) = do_get(crate::api::router(state), "/search?q=%20%20").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["messages"], serde_json::json!([]));
        assert_eq!(body["events"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn search_fts_finds_by_subject() {
        let state = make_state(true).await;
        seed(
            &state.pool,
            "m1",
            "acc1",
            "fld1",
            "Meeting tomorrow at noon",
            "alice@example.com",
            "Alice",
            "",
        )
        .await;
        let (status, body) = do_get(crate::api::router(state), "/search?q=tomorrow").await;
        assert_eq!(status, StatusCode::OK);
        let arr = body["messages"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "m1");
    }

    #[tokio::test]
    async fn search_fts_finds_by_from_email() {
        let state = make_state(true).await;
        seed(
            &state.pool,
            "m2",
            "acc1",
            "fld1",
            "Hello",
            "bob@company.com",
            "Bob",
            "preview",
        )
        .await;
        let (status, body) = do_get(crate::api::router(state), "/search?q=bob%40company.com").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "m2"));
    }

    #[tokio::test]
    async fn search_fts_no_match_returns_empty() {
        let state = make_state(true).await;
        seed(
            &state.pool,
            "m3",
            "acc1",
            "fld1",
            "Weekly report",
            "alice@example.com",
            "Alice",
            "",
        )
        .await;
        let (status, body) = do_get(crate::api::router(state), "/search?q=xyznonexistent").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["messages"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn search_fts_filtered_by_account_id() {
        let state = make_state(true).await;
        seed(
            &state.pool,
            "ma1",
            "acca",
            "flda",
            "Project update",
            "x@x.com",
            "X",
            "",
        )
        .await;
        seed(
            &state.pool,
            "mb1",
            "accb",
            "fldb",
            "Project update",
            "y@y.com",
            "Y",
            "",
        )
        .await;
        let (status, body) = do_get(
            crate::api::router(state),
            "/search?q=Project&account_id=acca",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let arr = body["messages"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "ma1");
    }

    #[tokio::test]
    async fn search_fts_finds_by_preview() {
        let state = make_state(true).await;
        seed(
            &state.pool,
            "m4",
            "acc1",
            "fld1",
            "No clue",
            "a@b.com",
            "A",
            "urgent action required",
        )
        .await;
        let (status, body) = do_get(crate::api::router(state), "/search?q=urgent").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "m4"));
    }

    #[tokio::test]
    async fn search_like_fallback_finds_by_subject() {
        let state = make_state(false).await;
        seed(
            &state.pool,
            "ml1",
            "acc1",
            "fld1",
            "Invoice for services",
            "vendor@example.com",
            "Vendor",
            "details",
        )
        .await;
        let (status, body) = do_get(crate::api::router(state), "/search?q=Invoice").await;
        assert_eq!(status, StatusCode::OK);
        let arr = body["messages"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "ml1");
    }

    #[tokio::test]
    async fn search_like_fallback_filtered_by_account_id() {
        let state = make_state(false).await;
        seed(
            &state.pool,
            "lka",
            "acca",
            "flda",
            "Budget 2025",
            "x@x.com",
            "X",
            "",
        )
        .await;
        seed(
            &state.pool,
            "lkb",
            "accb",
            "fldb",
            "Budget 2025",
            "y@y.com",
            "Y",
            "",
        )
        .await;
        let (status, body) = do_get(
            crate::api::router(state),
            "/search?q=Budget&account_id=accb",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let arr = body["messages"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "lkb");
    }

    #[tokio::test]
    async fn suggest_too_short_returns_empty() {
        let state = make_state(true).await;
        let (status, body) = do_get(crate::api::router(state), "/search/suggest?q=a").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["messages"], serde_json::json!([]));
        assert_eq!(body["events"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn suggest_fts_prefix_matches_multiple() {
        let state = make_state(true).await;
        seed(
            &state.pool,
            "s1",
            "acc1",
            "fld1",
            "Quarterly review",
            "mgr@corp.com",
            "Manager",
            "",
        )
        .await;
        seed(
            &state.pool,
            "s2",
            "acc1",
            "fld1",
            "Quarter budget",
            "fin@corp.com",
            "Finance",
            "",
        )
        .await;
        let (status, body) = do_get(crate::api::router(state), "/search/suggest?q=Quar").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn suggest_fts_capped_at_8_results() {
        let state = make_state(true).await;
        for i in 0..10 {
            seed(
                &state.pool,
                &format!("sg{}", i),
                "acc1",
                "fld1",
                &format!("Newsletter issue {}", i),
                "news@x.com",
                "News",
                "",
            )
            .await;
        }
        let (status, body) =
            do_get(crate::api::router(state), "/search/suggest?q=Newsletter").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["messages"].as_array().unwrap().len() <= 8);
    }

    #[tokio::test]
    async fn suggest_like_fallback_returns_matches() {
        let state = make_state(false).await;
        seed(
            &state.pool,
            "sl1",
            "acc1",
            "fld1",
            "Newsletter issue 42",
            "news@example.com",
            "News",
            "",
        )
        .await;
        let (status, body) = do_get(crate::api::router(state), "/search/suggest?q=News").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "sl1"));
    }
}
