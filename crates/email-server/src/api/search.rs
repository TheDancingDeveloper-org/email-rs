use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use email_types::Message as MessageRow;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub account_id: Option<String>,
    #[serde(rename = "match")]
    pub match_mode: Option<String>,
    pub from: Option<String>,
    pub from_op: Option<String>,
    pub to: Option<String>,
    pub to_op: Option<String>,
    pub cc: Option<String>,
    pub cc_op: Option<String>,
    pub subject: Option<String>,
    pub subject_op: Option<String>,
    pub body: Option<String>,
    pub body_op: Option<String>,
    pub filename: Option<String>,
    pub filename_op: Option<String>,
    pub has_attachment: Option<String>,
    pub is_read: Option<String>,
    pub is_flagged: Option<String>,
    pub date_after: Option<String>,
    pub date_before: Option<String>,
    /// `true` also asks each IMAP server to search its mailboxes, so messages
    /// the local index misses (a body not cached, an older sync window) are
    /// found when they are cached. Opt-in: it opens a connection per folder.
    pub remote: Option<String>,
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

const SEARCH_LIMIT: usize = 50;
/// Folders asked per account, and server-side matches kept per folder.
const REMOTE_FOLDER_LIMIT: i64 = 20;
const REMOTE_UID_LIMIT: usize = 50;

/// Append `more` after `messages` (already ranked), skipping repeats and
/// keeping the result within [`SEARCH_LIMIT`].
fn merge_remote(messages: &mut Vec<MessageRow>, more: Vec<MessageRow>) {
    let mut seen: std::collections::HashSet<String> =
        messages.iter().map(|m| m.id.clone()).collect();
    for message in more {
        if messages.len() >= SEARCH_LIMIT {
            break;
        }
        if seen.insert(message.id.clone()) {
            messages.push(message);
        }
    }
}

/// Ask each IMAP account's server (`UID SEARCH TEXT`) for `term` and return
/// the matches that are cached locally. A message outside the sync window is
/// not cached and can't be returned. A server or account that fails is
/// logged and skipped: the local results stand.
async fn search_remote(state: &AppState, term: &str, account_id: Option<&str>) -> Vec<MessageRow> {
    let accounts: Vec<String> = match sqlx::query_scalar(
        "SELECT id FROM accounts WHERE (?1 IS NULL OR id = ?1) ORDER BY id",
    )
    .bind(account_id)
    .fetch_all(&state.pool)
    .await
    {
        Ok(accounts) => accounts,
        Err(error) => {
            tracing::warn!(%error, "remote search could not list accounts");
            return Vec::new();
        }
    };
    let mut found = Vec::new();
    for account in accounts {
        let provider =
            match crate::providers::factory::provider_for_account(&state.pool, &account).await {
                Ok(provider) => provider,
                Err(error) => {
                    tracing::warn!(%error, account, "remote search skipped an account");
                    continue;
                }
            };
        let folders: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT id, full_path, uid_validity FROM folders WHERE account_id = ?
             ORDER BY total_count DESC LIMIT ?",
        )
        .bind(&account)
        .bind(REMOTE_FOLDER_LIMIT)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        for (folder_id, path, uid_validity) in folders {
            let uids = match provider.search_uids(&path, term).await {
                Ok(uids) => uids,
                Err(error) => {
                    tracing::warn!(%error, account, folder = path, "remote search failed");
                    continue;
                }
            };
            // Newest first: higher UIDs are newer.
            let mut uids = uids;
            uids.sort_unstable_by(|a, b| b.cmp(a));
            uids.truncate(REMOTE_UID_LIMIT);
            if uids.is_empty() {
                continue;
            }
            let placeholders = vec!["?"; uids.len()].join(", ");
            let sql = format!(
                r#"SELECT m.id, m.id AS occurrence_id, m.logical_id, m.account_id, m.folder_id, m.uid, m.message_id,
                          m.thread_id, m.subject, m.from_name, m.from_email, m.to_json,
                          m.date, m.is_read, m.is_flagged, m.is_draft, m.has_attachments, m.preview,
                          m.snoozed_until
                   FROM messages m
                   WHERE m.folder_id = ? AND m.uid_validity = ? AND m.uid IN ({placeholders})
                   ORDER BY m.date DESC"#
            );
            let mut query = sqlx::query_as::<_, MessageRow>(&sql)
                .bind(&folder_id)
                .bind(uid_validity);
            for uid in &uids {
                query = query.bind(i64::from(*uid));
            }
            match query.fetch_all(&state.pool).await {
                Ok(rows) => found.extend(rows),
                Err(error) => tracing::warn!(%error, "remote search lookup failed"),
            }
        }
    }
    found.sort_by(|a, b| b.date.cmp(&a.date));
    found
}

// ── Search index administration ──────────────────────────────────────────────

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IndexAdminResponse {
    pub status: &'static str,
    pub indexed: Option<i64>,
}

fn require_fts(state: &AppState) -> Result<()> {
    if state.has_fts {
        Ok(())
    } else {
        Err(AppError::Validation(
            "this database has no FTS5 search index".into(),
        ))
    }
}

/// `POST /admin/search-index/optimize`: merge the index segments now, as the
/// periodic maintenance does.
pub async fn optimize_index(
    State(state): State<Arc<AppState>>,
) -> Result<Json<IndexAdminResponse>> {
    require_fts(&state)?;
    crate::maintenance::optimize_fts(&state.pool).await?;
    Ok(Json(IndexAdminResponse {
        status: "optimized",
        indexed: None,
    }))
}

/// `POST /admin/search-index/rebuild`: re-create every index row from the
/// messages and attachments, then rewrite the clean bodies in the background.
/// Search covers subject, sender, recipients, preview and attachment names
/// at once and bodies as they are reindexed.
pub async fn rebuild_index(
    State(state): State<Arc<AppState>>,
) -> Result<(axum::http::StatusCode, Json<IndexAdminResponse>)> {
    require_fts(&state)?;
    let indexed = crate::maintenance::rebuild_fts(&state.pool).await?;
    crate::search_index::spawn_reindex(state.pool.clone());
    Ok((
        axum::http::StatusCode::ACCEPTED,
        Json(IndexAdminResponse {
            status: "rebuilding",
            indexed: Some(indexed),
        }),
    ))
}

// ── Full search ───────────────────────────────────────────────────────────────

pub async fn search_messages(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<SearchResponse>> {
    let term = params.q.as_deref().unwrap_or("").trim();
    let structured = structured_search_parts(&params);
    let has_structured = !structured.is_empty();
    if term.is_empty() && !has_structured {
        return Ok(Json(SearchResponse {
            messages: vec![],
            events: vec![],
        }));
    }

    let messages = if has_structured {
        search_structured(
            &state,
            params.account_id.as_deref(),
            &params.match_mode,
            structured,
        )
        .await?
    } else if state.has_fts {
        search_fts(&state, term, params.account_id.as_deref()).await?
    } else {
        search_like(&state, term, params.account_id.as_deref()).await?
    };
    let mut messages = messages;
    if !has_structured
        && !term.is_empty()
        && params.remote.as_deref() == Some("true")
        && messages.len() < SEARCH_LIMIT
    {
        let more = search_remote(&state, term, params.account_id.as_deref()).await;
        merge_remote(&mut messages, more);
    }
    let events = if term.is_empty() || has_structured {
        Vec::new()
    } else {
        search_calendar_events(&state, term, params.account_id.as_deref(), 20).await?
    };

    Ok(Json(SearchResponse { messages, events }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SearchBind {
    Bool(bool),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StructuredSearchPart {
    clause: String,
    binds: Vec<SearchBind>,
}

fn structured_search_parts(params: &SearchQuery) -> Vec<StructuredSearchPart> {
    let mut parts = Vec::new();
    push_text_search_part(
        &mut parts,
        "COALESCE(m.from_name, '') || ' ' || COALESCE(m.from_email, '')",
        params.from.as_deref(),
        params.from_op.as_deref(),
    );
    push_text_search_part(
        &mut parts,
        "COALESCE(m.to_json, '')",
        params.to.as_deref(),
        params.to_op.as_deref(),
    );
    push_text_search_part(
        &mut parts,
        "COALESCE(m.cc_json, '')",
        params.cc.as_deref(),
        params.cc_op.as_deref(),
    );
    push_text_search_part(
        &mut parts,
        "COALESCE(m.subject, '')",
        params.subject.as_deref(),
        params.subject_op.as_deref(),
    );
    // The explicit body filter matches the raw MIME parts on purpose: a
    // structured search finds markup such as schema.org (`body=schema.org`) in
    // HTML the clean body drops. Free-text `q` searches the clean body.
    push_text_search_part(
        &mut parts,
        "COALESCE(m.preview, '') || ' ' || COALESCE(mb.text_body, '') || ' ' || COALESCE(mb.html_body, '')",
        params.body.as_deref(),
        params.body_op.as_deref(),
    );
    push_filename_search_part(
        &mut parts,
        params.filename.as_deref(),
        params.filename_op.as_deref(),
    );
    push_bool_search_part(
        &mut parts,
        "m.has_attachments",
        params.has_attachment.as_deref(),
    );
    push_bool_search_part(&mut parts, "m.is_read", params.is_read.as_deref());
    push_bool_search_part(&mut parts, "m.is_flagged", params.is_flagged.as_deref());
    push_date_search_part(&mut parts, ">", params.date_after.as_deref());
    push_date_search_part(&mut parts, "<", params.date_before.as_deref());
    parts
}

fn push_text_search_part(
    parts: &mut Vec<StructuredSearchPart>,
    expression: &'static str,
    value: Option<&str>,
    operator: Option<&str>,
) {
    let Some(value) = trimmed_non_empty(value) else {
        return;
    };

    let operator = normalized_text_operator(operator);
    let (clause, bind) = text_search_clause(expression, operator, &value);
    parts.push(StructuredSearchPart {
        clause,
        binds: vec![SearchBind::Text(bind)],
    });
}

fn push_filename_search_part(
    parts: &mut Vec<StructuredSearchPart>,
    value: Option<&str>,
    operator: Option<&str>,
) {
    let Some(value) = trimmed_non_empty(value) else {
        return;
    };

    let operator = normalized_text_operator(operator);
    let exists_operator = if operator == "not_contains" {
        "contains"
    } else {
        operator
    };
    let (text_clause, bind) = text_search_clause("a.filename", exists_operator, &value);
    let clause = if operator == "not_contains" {
        format!(
            "NOT EXISTS (SELECT 1 FROM attachments a WHERE a.message_id = m.id AND {})",
            text_clause
        )
    } else {
        format!(
            "EXISTS (SELECT 1 FROM attachments a WHERE a.message_id = m.id AND {})",
            text_clause
        )
    };
    parts.push(StructuredSearchPart {
        clause,
        binds: vec![SearchBind::Text(bind)],
    });
}

fn push_bool_search_part(
    parts: &mut Vec<StructuredSearchPart>,
    column: &'static str,
    value: Option<&str>,
) {
    let Some(value) = parse_search_bool(value) else {
        return;
    };
    parts.push(StructuredSearchPart {
        clause: format!("{column} = ?"),
        binds: vec![SearchBind::Bool(value)],
    });
}

fn push_date_search_part(
    parts: &mut Vec<StructuredSearchPart>,
    operator: &'static str,
    value: Option<&str>,
) {
    let Some(value) = trimmed_non_empty(value) else {
        return;
    };
    parts.push(StructuredSearchPart {
        clause: format!("m.date IS NOT NULL AND substr(m.date, 1, 10) {operator} ?"),
        binds: vec![SearchBind::Text(value)],
    });
}

fn trimmed_non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn parse_search_bool(value: Option<&str>) -> Option<bool> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

fn normalized_text_operator(operator: Option<&str>) -> &'static str {
    match operator.unwrap_or("contains") {
        "not_contains" => "not_contains",
        "equals" => "equals",
        "starts_with" => "starts_with",
        _ => "contains",
    }
}

fn text_search_clause(
    expression: &'static str,
    operator: &'static str,
    value: &str,
) -> (String, String) {
    match operator {
        "not_contains" => (
            format!("lower({expression}) NOT LIKE lower(?) ESCAPE '\\'"),
            format!("%{}%", escape_like(value)),
        ),
        "equals" => (format!("lower({expression}) = lower(?)"), value.to_string()),
        "starts_with" => (
            format!("lower({expression}) LIKE lower(?) ESCAPE '\\'"),
            format!("{}%", escape_like(value)),
        ),
        _ => (
            format!("lower({expression}) LIKE lower(?) ESCAPE '\\'"),
            format!("%{}%", escape_like(value)),
        ),
    }
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

async fn search_structured(
    state: &AppState,
    account_id: Option<&str>,
    match_mode: &Option<String>,
    parts: Vec<StructuredSearchPart>,
) -> Result<Vec<MessageRow>> {
    let joiner = if match_mode.as_deref() == Some("any") {
        " OR "
    } else {
        " AND "
    };
    let structured_clause = parts
        .iter()
        .map(|part| format!("({})", part.clause))
        .collect::<Vec<_>>()
        .join(joiner);
    let account_clause = if account_id.is_some() {
        " AND m.account_id = ?"
    } else {
        ""
    };
    let sql = format!(
        r#"SELECT m.id, m.id AS occurrence_id, m.logical_id, m.account_id, m.folder_id, m.uid, m.message_id,
                  m.thread_id, m.subject, m.from_name, m.from_email, m.to_json,
                  m.date, m.is_read, m.is_flagged, m.is_draft, m.has_attachments, m.preview,
                  m.snoozed_until
           FROM messages m
           LEFT JOIN message_bodies mb ON mb.message_id = m.id
           WHERE ({structured_clause}){account_clause}
           GROUP BY m.id
           ORDER BY m.date DESC
           LIMIT 50"#
    );

    let mut query = sqlx::query_as::<_, MessageRow>(&sql);
    for part in parts {
        for bind in part.binds {
            query = match bind {
                SearchBind::Bool(value) => query.bind(value),
                SearchBind::Text(value) => query.bind(value),
            };
        }
    }
    if let Some(account_id) = account_id {
        query = query.bind(account_id);
    }

    query.fetch_all(&state.pool).await.map_err(Into::into)
}

async fn search_fts(
    state: &AppState,
    term: &str,
    account_id: Option<&str>,
) -> Result<Vec<MessageRow>> {
    let q = fts_query(term);

    // Column weights: message_id(unindexed)=0, subject=10, sender=5,
    // recipients=2, body (the clean current message)=2, quoted history=0.2,
    // attachment names=3.
    // Quoted history stays searchable so a reply that quotes mail we don't
    // hold can still be found, but a message that only quotes a term ranks
    // below the messages that say it.
    let rows = if let Some(aid) = account_id {
        sqlx::query_as::<_, MessageRow>(
            r#"SELECT m.id, m.id AS occurrence_id, m.logical_id, m.account_id, m.folder_id, m.uid, m.message_id,
                      m.thread_id, m.subject, m.from_name, m.from_email, m.to_json,
                      m.date, m.is_read, m.is_flagged, m.is_draft, m.has_attachments, m.preview,
                      m.snoozed_until
               FROM messages_fts fts
               JOIN messages m ON m.id = fts.message_id
               WHERE messages_fts MATCH ? AND m.account_id = ?
               ORDER BY bm25(messages_fts, 0, 10, 5, 2, 2, 0.2, 3)
               LIMIT 50"#,
        )
        .bind(&q)
        .bind(aid)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, MessageRow>(
            r#"SELECT m.id, m.id AS occurrence_id, m.logical_id, m.account_id, m.folder_id, m.uid, m.message_id,
                      m.thread_id, m.subject, m.from_name, m.from_email, m.to_json,
                      m.date, m.is_read, m.is_flagged, m.is_draft, m.has_attachments, m.preview,
                      m.snoozed_until
               FROM messages_fts fts
               JOIN messages m ON m.id = fts.message_id
               WHERE messages_fts MATCH ?
               ORDER BY bm25(messages_fts, 0, 10, 5, 2, 2, 0.2, 3)
               LIMIT 50"#,
        )
        .bind(&q)
        .fetch_all(&state.pool)
        .await?
    };

    Ok(rows)
}

/// The LIKE fallback when SQLite has no FTS5. It searches what the FTS index
/// holds (subject, sender, recipients, preview, the clean current message and
/// its quoted history) and ranks alike: a match only in quoted history comes
/// last. Unlike the index it can't drop link targets from the body.
async fn search_like(
    state: &AppState,
    term: &str,
    account_id: Option<&str>,
) -> Result<Vec<MessageRow>> {
    let term = escape_like(term);
    let q_sub = format!("%{term}%");
    let q_word_start = format!("{term} %");
    let q_word_mid = format!("% {term}%");
    let account_clause = if account_id.is_some() {
        "m.account_id = ? AND "
    } else {
        ""
    };
    let sql = format!(
        r#"SELECT m.id, m.id AS occurrence_id, m.logical_id, m.account_id, m.folder_id, m.uid, m.message_id,
                  m.thread_id, m.subject, m.from_name, m.from_email, m.to_json,
                  m.date, m.is_read, m.is_flagged, m.is_draft, m.has_attachments, m.preview,
                  m.snoozed_until
           FROM messages m
           LEFT JOIN message_bodies mb ON mb.message_id = m.id
           WHERE {account_clause}
                 (m.subject LIKE ? ESCAPE '\' OR m.from_name LIKE ? ESCAPE '\' OR m.from_email LIKE ? ESCAPE '\'
                  OR m.to_json LIKE ? ESCAPE '\' OR m.cc_json LIKE ? ESCAPE '\' OR m.preview LIKE ? ESCAPE '\'
                  OR mb.clean_body LIKE ? ESCAPE '\' OR mb.quoted_history LIKE ? ESCAPE '\'
                  OR EXISTS (SELECT 1 FROM attachments a
                              WHERE a.message_id = m.id AND a.filename LIKE ? ESCAPE '\'))
           GROUP BY m.id
           ORDER BY CASE
                      WHEN lower(m.subject) LIKE lower(?) ESCAPE '\' OR lower(m.subject) LIKE lower(?) ESCAPE '\' THEN 4
                      WHEN m.subject LIKE ? ESCAPE '\' THEN 3
                      WHEN m.from_name LIKE ? ESCAPE '\' OR m.from_email LIKE ? ESCAPE '\' THEN 2
                      WHEN m.to_json LIKE ? ESCAPE '\' OR m.cc_json LIKE ? ESCAPE '\' OR m.preview LIKE ? ESCAPE '\'
                           OR mb.clean_body LIKE ? ESCAPE '\' THEN 1
                      ELSE 0 END DESC,
                    m.date DESC
           LIMIT 50"#
    );

    let mut query = sqlx::query_as::<_, MessageRow>(&sql);
    if let Some(aid) = account_id {
        query = query.bind(aid);
    }
    for _ in 0..9 {
        query = query.bind(q_sub.clone());
    }
    query = query.bind(q_word_start).bind(q_word_mid);
    for _ in 0..8 {
        query = query.bind(q_sub.clone());
    }
    Ok(query.fetch_all(&state.pool).await?)
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
    let term = params.q.as_deref().unwrap_or("").trim();
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
                   ORDER BY bm25(messages_fts, 0, 10, 5, 2, 2, 0.2, 3)
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
                   ORDER BY bm25(messages_fts, 0, 10, 5, 2, 2, 0.2, 3)
                   LIMIT 8"#,
            )
            .bind(&q)
            .fetch_all(&state.pool)
            .await?
        }
    } else {
        // Prefix LIKE on subject/from only — no leading wildcard so index is usable
        let term = escape_like(term);
        let q_prefix = format!("{term}%");
        let q_sub = format!("%{term}%");
        sqlx::query_as::<_, SuggestRow>(
            r#"SELECT id, folder_id, subject, from_name, from_email, date
               FROM messages
               WHERE subject LIKE ? ESCAPE '\' OR from_name LIKE ? ESCAPE '\' OR from_email LIKE ? ESCAPE '\'
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
    let q = format!("%{}%", escape_like(term));
    sqlx::query_as::<_, CalendarSuggestRow>(
        "SELECT id, title, start_at FROM calendar_events \
         WHERE title LIKE ? ESCAPE '\' OR description LIKE ? ESCAPE '\' \
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
    let q = format!("%{}%", escape_like(term));
    if let Some(aid) = account_id {
        sqlx::query_as::<_, CalendarSearchRow>(
            "SELECT id, title, description, start_at, end_at, location, is_all_day \
             FROM calendar_events \
             WHERE account_id = ? AND (title LIKE ? ESCAPE '\' OR description LIKE ? ESCAPE '\') \
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
             WHERE title LIKE ? ESCAPE '\' OR description LIKE ? ESCAPE '\' \
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
    let q = format!("%{}%", escape_like(term));
    let rows = sqlx::query_as::<_, ContactRow>(
        r#"SELECT from_name AS name, from_email AS email
           FROM messages
           WHERE from_email IS NOT NULL
             AND (from_email LIKE ? ESCAPE '\' OR from_name LIKE ? ESCAPE '\')
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
        let _db_dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let path = _db_dir.path().join("email.db").display().to_string();
        let (pool, has_fts) = crate::db::create_pool(&format!("sqlite:{path}"))
            .await
            .unwrap();
        Arc::new(crate::state::AppState::new(pool, has_fts && with_fts))
    }

    #[allow(clippy::too_many_arguments)]
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
        .bind(format!("{account_id}@example.com"))
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
        let uid: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(uid), 0) + 1 FROM messages WHERE folder_id = ?",
        )
        .bind(folder_id)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (id, account_id, folder_id, uid, subject, from_email, from_name, preview, message_id) VALUES (?,?,?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind(account_id)
        .bind(folder_id)
        .bind(uid)
        .bind(subject)
        .bind(from_email)
        .bind(from_name)
        .bind(preview)
        .bind(format!("<{id}>"))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_attachment(pool: &sqlx::SqlitePool, message_id: &str, filename: &str) {
        sqlx::query(
            "INSERT INTO attachments (id, message_id, filename, content_type, part_id, size_bytes) VALUES (?,?,?,?,?,?)",
        )
        .bind(format!("{message_id}-attachment"))
        .bind(message_id)
        .bind(filename)
        .bind("text/plain")
        .bind(format!("{message_id}-part"))
        .bind(12i64)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("UPDATE messages SET has_attachments = 1 WHERE id = ?")
            .bind(message_id)
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
            "bob@example.com",
            "Bob",
            "preview",
        )
        .await;
        let (status, body) = do_get(crate::api::router(state), "/search?q=bob%40example.com").await;
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
            "x@example.com",
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
            "y@example.net",
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
            "a@example.com",
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
    async fn search_like_fallback_treats_like_wildcards_as_literals() {
        let state = make_state(false).await;
        for (id, subject) in [
            ("percent", "100% complete"),
            ("underscore", "under_score"),
            ("slash", r"path\name"),
        ] {
            seed(
                &state.pool,
                id,
                "acc1",
                "fld1",
                subject,
                "sender@example.com",
                "Sender",
                "",
            )
            .await;
        }

        for (term, expected) in [("%25", "percent"), ("_", "underscore"), (r"\", "slash")] {
            assert_eq!(
                ids(state.clone(), &format!("/search?q={term}")).await,
                [expected]
            );
        }
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
            "x@example.com",
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
            "y@example.net",
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
    async fn structured_search_filters_without_q() {
        let state = make_state(false).await;
        seed(
            &state.pool,
            "structured-attach",
            "acc1",
            "fld1",
            "Attachment",
            "files@example.com",
            "Files",
            "",
        )
        .await;
        seed_attachment(&state.pool, "structured-attach", "invoice.pdf").await;
        seed(
            &state.pool,
            "structured-plain",
            "acc1",
            "fld1",
            "Plain",
            "plain@example.com",
            "Plain",
            "",
        )
        .await;

        let (status, body) = do_get(crate::api::router(state), "/search?has_attachment=true").await;

        assert_eq!(status, StatusCode::OK);
        let arr = body["messages"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "structured-attach");
        assert_eq!(body["events"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn structured_search_applies_match_all_and_text_operator() {
        let state = make_state(false).await;
        seed(
            &state.pool,
            "structured-bob-quarter",
            "acc1",
            "fld1",
            "Quarter report",
            "bob@example.com",
            "Bob Builder",
            "",
        )
        .await;
        seed(
            &state.pool,
            "structured-alice-quarter",
            "acc1",
            "fld1",
            "Quarter report",
            "alice@example.com",
            "Alice",
            "",
        )
        .await;
        seed(
            &state.pool,
            "structured-bob-daily",
            "acc1",
            "fld1",
            "Daily note",
            "bob@example.com",
            "Bob Builder",
            "",
        )
        .await;

        let (status, body) = do_get(
            crate::api::router(state),
            "/search?match=all&from=bo&from_op=starts_with&subject=quarter",
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let arr = body["messages"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "structured-bob-quarter");
    }

    #[tokio::test]
    async fn structured_search_applies_match_any_booleans_and_dates() {
        let state = make_state(false).await;
        seed(
            &state.pool,
            "structured-flagged",
            "acc1",
            "fld1",
            "Flagged",
            "flag@example.com",
            "Flag",
            "",
        )
        .await;
        sqlx::query("UPDATE messages SET is_flagged = 1, date = '2026-01-01T00:00:00Z' WHERE id = 'structured-flagged'")
            .execute(&state.pool)
            .await
            .unwrap();
        seed(
            &state.pool,
            "structured-old",
            "acc1",
            "fld1",
            "Old",
            "old@example.com",
            "Old",
            "",
        )
        .await;
        sqlx::query(
            "UPDATE messages SET date = '2019-12-31T23:59:00Z' WHERE id = 'structured-old'",
        )
        .execute(&state.pool)
        .await
        .unwrap();
        seed(
            &state.pool,
            "structured-neither",
            "acc1",
            "fld1",
            "Neither",
            "neither@example.com",
            "Neither",
            "",
        )
        .await;
        sqlx::query(
            "UPDATE messages SET date = '2026-01-01T00:00:00Z' WHERE id = 'structured-neither'",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let (status, body) = do_get(
            crate::api::router(state),
            "/search?match=any&is_flagged=true&date_before=2020-01-01",
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let ids = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|message| message["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"structured-flagged"));
        assert!(ids.contains(&"structured-old"));
    }

    #[tokio::test]
    async fn structured_search_filters_filename_and_account() {
        let state = make_state(false).await;
        seed(
            &state.pool,
            "structured-acc1-attachment",
            "acc1",
            "fld1",
            "Attachment",
            "a@example.com",
            "A",
            "",
        )
        .await;
        seed_attachment(&state.pool, "structured-acc1-attachment", "budget.xlsx").await;
        seed(
            &state.pool,
            "structured-acc2-attachment",
            "acc2",
            "fld2",
            "Attachment",
            "b@example.com",
            "B",
            "",
        )
        .await;
        seed_attachment(&state.pool, "structured-acc2-attachment", "budget.xlsx").await;

        let (status, body) = do_get(
            crate::api::router(state),
            "/search?filename=budget&account_id=acc2",
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let arr = body["messages"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "structured-acc2-attachment");
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
            "mgr@example.com",
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
            "fin@example.com",
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
                "news@example.com",
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

    // ── Clean-body search ───────────────────────────────────────────────────────

    /// Cache a body the way sync does: store the MIME parts, then clean (and
    /// index) them.
    async fn seed_body(pool: &sqlx::SqlitePool, id: &str, text: Option<&str>, html: Option<&str>) {
        sqlx::query("INSERT INTO message_bodies (message_id, text_body, html_body) VALUES (?,?,?)")
            .bind(id)
            .bind(text)
            .bind(html)
            .execute(pool)
            .await
            .unwrap();
        crate::clean::store(pool, id, text, html).await.unwrap();
    }

    const SAYS_WALRUS: &str = "Hi Tom,\n\nThe walrus budget is approved.\n\nPriya\n\nOn Mon, 28 Sep 2026 at 17:02, Tom Baker <tom@example.com> wrote:\n> Any news on the venue?\n";
    const QUOTES_WALRUS: &str = "Sounds good, see you then.\n\nOn Mon, 28 Sep 2026 at 17:02, Tom Baker <tom@example.com> wrote:\n> The walrus budget is still pending.\n> Tom\n";
    const HTML_WITH_NOISE: &str = r#"<html><head><style>.flamingoclass{color:red}</style></head><body>
        <div class="flamingoclass" data-campaign="pangolinattr"><p>Your parcel has shipped.</p>
        <p><a href="https://shop.example/orders/zebracode?ref=mail">Track your parcel</a></p>
        <img alt="ocelotpic" src="https://cdn.example/ocelot.png"></div></body></html>"#;

    async fn seed_clean_corpus(pool: &sqlx::SqlitePool) {
        seed(
            pool,
            "says",
            "acc1",
            "fld1",
            "Venue",
            "priya@example.com",
            "Priya",
            "",
        )
        .await;
        seed_body(pool, "says", Some(SAYS_WALRUS), None).await;
        seed(
            pool,
            "quotes",
            "acc1",
            "fld1",
            "Venue",
            "sam@example.com",
            "Sam",
            "",
        )
        .await;
        seed_body(pool, "quotes", Some(QUOTES_WALRUS), None).await;
        seed(
            pool,
            "html",
            "acc1",
            "fld1",
            "Dispatch",
            "orders@shop.example",
            "Shop Orders",
            "",
        )
        .await;
        seed_body(pool, "html", None, Some(HTML_WITH_NOISE)).await;
        // Unrelated mail so term statistics are not degenerate.
        for i in 0..5 {
            seed(
                pool,
                &format!("other{i}"),
                "acc1",
                "fld1",
                "Weekly note",
                "n@example.com",
                "N",
                "",
            )
            .await;
            seed_body(
                pool,
                &format!("other{i}"),
                Some("Nothing to see here."),
                None,
            )
            .await;
        }
    }

    async fn ids(state: Arc<crate::state::AppState>, uri: &str) -> Vec<String> {
        let (status, body) = do_get(crate::api::router(state), uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn quoted_history_is_split_off_in_the_fixture() {
        let cleaned = crate::clean::clean(Some(QUOTES_WALRUS), None, Some("Venue"));
        assert!(!cleaned.body.contains("walrus"), "{}", cleaned.body);
        assert!(cleaned.quoted_history.unwrap().contains("walrus"));
    }

    #[tokio::test]
    async fn fts_ranks_a_quoted_only_match_below_the_clean_body() {
        let state = make_state(true).await;
        seed_clean_corpus(&state.pool).await;
        assert_eq!(ids(state, "/search?q=walrus").await, ["says", "quotes"]);
    }

    #[tokio::test]
    async fn fts_matches_the_whole_clean_body_not_only_the_preview() {
        let state = make_state(true).await;
        seed(
            &state.pool,
            "long",
            "acc1",
            "fld1",
            "Notes",
            "a@example.com",
            "A",
            "",
        )
        .await;
        let text = format!(
            "{}\n\nThe capybara arrives on Friday.",
            "Filler words. ".repeat(40)
        );
        seed_body(&state.pool, "long", Some(&text), None).await;
        assert_eq!(ids(state, "/search?q=capybara").await, ["long"]);
    }

    #[tokio::test]
    async fn fts_ignores_html_markup_attributes_and_link_targets() {
        let state = make_state(true).await;
        seed_clean_corpus(&state.pool).await;
        for term in [
            "flamingoclass",
            "pangolinattr",
            "zebracode",
            "ocelotpic",
            "ocelot",
        ] {
            assert!(
                ids(state.clone(), &format!("/search?q={term}"))
                    .await
                    .is_empty(),
                "{term}"
            );
        }
        assert_eq!(ids(state.clone(), "/search?q=shipped").await, ["html"]);
        assert_eq!(ids(state, "/search?q=Track%20parcel").await, ["html"]);
    }

    #[tokio::test]
    async fn fts_sender_and_recipient_search_still_work() {
        let state = make_state(true).await;
        seed_clean_corpus(&state.pool).await;
        sqlx::query(
            r#"UPDATE messages SET to_json = '["Jo Bloggs <jo@example.org>"]',
                                   cc_json = '[{"name":"Kim Lee","email":"kim@example.net"}]'
               WHERE id = 'says'"#,
        )
        .execute(&state.pool)
        .await
        .unwrap();
        // An address list that isn't JSON is indexed as it is, not refused.
        sqlx::query("UPDATE messages SET to_json = 'wombat@example.org' WHERE id = 'quotes'")
            .execute(&state.pool)
            .await
            .unwrap();
        assert_eq!(
            ids(state.clone(), "/search?q=orders%40shop.example").await,
            ["html"]
        );
        assert_eq!(
            ids(state.clone(), "/search?q=Shop%20Orders").await,
            ["html"]
        );
        assert_eq!(ids(state.clone(), "/search?q=Bloggs").await, ["says"]);
        assert_eq!(
            ids(state.clone(), "/search?q=kim%40example.net").await,
            ["says"]
        );
        assert_eq!(ids(state.clone(), "/search?q=wombat").await, ["quotes"]);
        // JSON keys are not indexed.
        assert!(ids(state.clone(), "/search?q=name").await.is_empty());
        // Structured sender search uses the structured filter.
        assert_eq!(
            ids(
                state.clone(),
                "/search?from=orders%40shop.example&account_id=acc1"
            )
            .await,
            ["html"]
        );
    }

    #[tokio::test]
    async fn fts_keeps_the_clean_body_when_the_preview_changes() {
        let state = make_state(true).await;
        seed_clean_corpus(&state.pool).await;
        // A later sync writes the raw provider preview back.
        sqlx::query("UPDATE messages SET preview = 'raw zebracode preview' WHERE id = 'html'")
            .execute(&state.pool)
            .await
            .unwrap();
        assert!(ids(state.clone(), "/search?q=zebracode").await.is_empty());
        assert_eq!(ids(state, "/search?q=shipped").await, ["html"]);
    }

    #[tokio::test]
    async fn reindex_fills_bodies_cleaned_before_the_index() {
        let state = make_state(true).await;
        seed_clean_corpus(&state.pool).await;
        // Bodies cleaned, the index holding previews only.
        sqlx::query("UPDATE messages_fts SET body = '', quoted = ''")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE message_bodies SET search_version = 0")
            .execute(&state.pool)
            .await
            .unwrap();
        assert!(ids(state.clone(), "/search?q=walrus").await.is_empty());

        assert_eq!(crate::search_index::reindex(&state.pool).await.unwrap(), 8);
        assert_eq!(
            ids(state.clone(), "/search?q=walrus").await,
            ["says", "quotes"]
        );
        assert_eq!(crate::search_index::reindex(&state.pool).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn first_index_is_replaced_and_bodies_reindexed() {
        let _db_dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let path = _db_dir.path().join("email.db").display().to_string();
        let url = format!("sqlite:{path}");
        let (pool, has_fts) = crate::db::create_pool(&url).await.unwrap();
        assert!(has_fts);
        seed_clean_corpus(&pool).await;
        // Put back the first index (raw previews, no bodies).
        for statement in [
            "DROP TRIGGER messages_fts_ai",
            "DROP TRIGGER messages_fts_ad",
            "DROP TRIGGER messages_fts_au",
            "DROP TRIGGER messages_fts_au_preview",
            "DROP TABLE messages_fts",
            "CREATE VIRTUAL TABLE messages_fts USING fts5(message_id UNINDEXED, subject, from_name, from_email, preview, tokenize = 'unicode61')",
            "INSERT INTO messages_fts(rowid, message_id, subject, from_name, from_email, preview) SELECT rowid, id, subject, from_name, from_email, preview FROM messages",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool.close().await;

        let (pool, has_fts) = crate::db::create_pool(&url).await.unwrap();
        assert!(has_fts);
        let pending: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM message_bodies WHERE search_version = 0")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(pending, 8);
        let indexed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages_fts")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(indexed, 8);
        assert_eq!(crate::search_index::reindex(&pool).await.unwrap(), 8);
        let state = Arc::new(crate::state::AppState::new(pool, true));
        assert_eq!(
            ids(state.clone(), "/search?q=walrus").await,
            ["says", "quotes"]
        );
        assert_eq!(ids(state, "/search?q=Priya").await, ["says"]);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn like_fallback_searches_the_clean_body_and_ranks_quotes_last() {
        let state = make_state(false).await;
        seed_clean_corpus(&state.pool).await;
        assert_eq!(
            ids(state.clone(), "/search?q=walrus").await,
            ["says", "quotes"]
        );
        assert_eq!(ids(state.clone(), "/search?q=shipped").await, ["html"]);
        for term in ["flamingoclass", "pangolinattr", "ocelotpic"] {
            assert!(
                ids(state.clone(), &format!("/search?q={term}"))
                    .await
                    .is_empty(),
                "{term}"
            );
        }
        assert_eq!(
            ids(state, "/search?q=orders%40shop.example&account_id=acc1").await,
            ["html"]
        );
    }

    async fn do_post(router: axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }

    #[tokio::test]
    async fn fts_indexes_attachment_filenames_as_they_change() {
        let state = make_state(true).await;
        seed(
            &state.pool,
            "att",
            "acc1",
            "fld1",
            "Contract",
            "a@example.com",
            "A",
            "",
        )
        .await;
        seed(
            &state.pool,
            "plain",
            "acc1",
            "fld1",
            "Lunch",
            "b@example.com",
            "B",
            "",
        )
        .await;
        assert!(ids(state.clone(), "/search?q=quarterlyledger")
            .await
            .is_empty());

        seed_attachment(&state.pool, "att", "quarterlyledger.xlsx").await;
        assert_eq!(
            ids(state.clone(), "/search?q=quarterlyledger").await,
            ["att"]
        );

        sqlx::query(
            "UPDATE attachments SET filename = 'renamedsheet.xlsx' WHERE message_id = 'att'",
        )
        .execute(&state.pool)
        .await
        .unwrap();
        assert!(ids(state.clone(), "/search?q=quarterlyledger")
            .await
            .is_empty());
        assert_eq!(ids(state.clone(), "/search?q=renamedsheet").await, ["att"]);

        sqlx::query("DELETE FROM attachments WHERE message_id = 'att'")
            .execute(&state.pool)
            .await
            .unwrap();
        assert!(ids(state, "/search?q=renamedsheet").await.is_empty());
    }

    #[tokio::test]
    async fn like_fallback_finds_attachment_filenames_and_still_escapes_wildcards() {
        let state = make_state(false).await;
        seed(
            &state.pool,
            "a1",
            "acc1",
            "fld1",
            "One",
            "a@example.com",
            "A",
            "",
        )
        .await;
        seed(
            &state.pool,
            "a2",
            "acc1",
            "fld1",
            "Two",
            "b@example.com",
            "B",
            "",
        )
        .await;
        seed_attachment(&state.pool, "a1", "budget_final.pdf").await;
        assert_eq!(ids(state.clone(), "/search?q=budget_final").await, ["a1"]);
        // `_` is a literal, not a one-character wildcard (#18).
        assert!(ids(state.clone(), "/search?q=budget-final")
            .await
            .is_empty());
        assert!(ids(state, "/search?q=budget%25").await.is_empty());
    }

    #[tokio::test]
    async fn rebuild_endpoint_restores_a_damaged_index_with_attachment_names() {
        let state = make_state(true).await;
        seed(
            &state.pool,
            "r1",
            "acc1",
            "fld1",
            "Rebuild me",
            "a@example.com",
            "A",
            "",
        )
        .await;
        seed_attachment(&state.pool, "r1", "stalefile.zip").await;
        sqlx::query("DELETE FROM messages_fts")
            .execute(&state.pool)
            .await
            .unwrap();
        assert!(ids(state.clone(), "/search?q=stalefile").await.is_empty());

        let (status, body) = do_post(
            crate::api::router(state.clone()),
            "/admin/search-index/rebuild",
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(body["indexed"], 1);
        assert_eq!(ids(state.clone(), "/search?q=stalefile").await, ["r1"]);
        assert_eq!(ids(state, "/search?q=rebuild").await, ["r1"]);
    }

    #[tokio::test]
    async fn optimize_endpoint_runs_and_needs_fts() {
        let state = make_state(true).await;
        let (status, body) =
            do_post(crate::api::router(state), "/admin/search-index/optimize").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "optimized");

        let no_fts = make_state(false).await;
        let (status, _) = do_post(
            crate::api::router(no_fts.clone()),
            "/admin/search-index/optimize",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = do_post(crate::api::router(no_fts), "/admin/search-index/rebuild").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn remote_search_adds_server_matches_that_the_index_misses() {
        let state = make_state(true).await;
        seed(
            &state.pool,
            "srv",
            "acc1",
            "fld1",
            "Unrelated",
            "a@example.com",
            "A",
            "",
        )
        .await;
        sqlx::query("UPDATE accounts SET provider_type = 'test' WHERE id = 'acc1'")
            .execute(&state.pool)
            .await
            .unwrap();
        // The local index has nothing for the term; only the server does.
        assert!(ids(state.clone(), "/search?q=serversideonly")
            .await
            .is_empty());
        assert!(ids(state.clone(), "/search?q=serversideonly&remote=false")
            .await
            .is_empty());
        assert_eq!(
            ids(state.clone(), "/search?q=serversideonly&remote=true").await,
            ["srv"]
        );
        assert_eq!(
            ids(
                state,
                "/search?q=serversideonly&remote=true&account_id=none"
            )
            .await,
            Vec::<String>::new()
        );
    }
}
