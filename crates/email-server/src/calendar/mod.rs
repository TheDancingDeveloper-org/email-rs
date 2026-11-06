pub mod sync;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::Result;
use crate::providers::{Attendee, CalendarEvent};

// ── DB row type ───────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct CalendarEventRow {
    id: String,
    #[allow(dead_code)]
    account_id: String,
    calendar_id: String,
    provider_event_id: Option<String>,
    title: String,
    description: Option<String>,
    start_at: String,
    end_at: String,
    location: Option<String>,
    is_all_day: bool,
    recurrence_rule: Option<String>,
    attendees_json: Option<String>,
    meet_link: Option<String>,
}

impl CalendarEventRow {
    fn into_event(self) -> CalendarEvent {
        CalendarEvent {
            id: self.id,
            calendar_id: self.calendar_id,
            provider_event_id: self.provider_event_id,
            title: self.title,
            description: self.description,
            start_at: self
                .start_at
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
            end_at: self
                .end_at
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now()),
            location: self.location,
            is_all_day: self.is_all_day,
            recurrence_rule: self.recurrence_rule,
            attendees: self
                .attendees_json
                .as_deref()
                .and_then(|j| serde_json::from_str::<Vec<Attendee>>(j).ok())
                .unwrap_or_default(),
            meet_link: self.meet_link,
        }
    }
}

// ── EventLink ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventLink {
    pub id: String,
    pub event_id: String,
    pub message_id: String,
    pub folder_id: String,
    pub subject: Option<String>,
    pub from_name: Option<String>,
    pub from_email: Option<String>,
    pub date: Option<String>,
    pub linked_at: String,
}

// ── CalendarService ───────────────────────────────────────────────────────────

pub struct CalendarService {
    pool: SqlitePool,
}

impl CalendarService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_events(
        &self,
        account_id: Option<&str>,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<CalendarEvent>> {
        let from_str = from.to_rfc3339();
        let to_str = to.to_rfc3339();

        let rows: Vec<CalendarEventRow> = if let Some(aid) = account_id {
            sqlx::query_as::<_, CalendarEventRow>(
                r#"SELECT id, account_id, calendar_id, provider_event_id, title, description,
                          start_at, end_at, location, is_all_day, recurrence_rule, attendees_json, meet_link
                   FROM calendar_events
                   WHERE account_id = ? AND start_at < ? AND end_at > ?
                   ORDER BY start_at"#,
            )
            .bind(aid)
            .bind(&to_str)
            .bind(&from_str)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, CalendarEventRow>(
                r#"SELECT id, account_id, calendar_id, provider_event_id, title, description,
                          start_at, end_at, location, is_all_day, recurrence_rule, attendees_json, meet_link
                   FROM calendar_events
                   WHERE start_at < ? AND end_at > ?
                   ORDER BY start_at"#,
            )
            .bind(&to_str)
            .bind(&from_str)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(rows.into_iter().map(|r| r.into_event()).collect())
    }

    pub async fn get_event(&self, id: &str) -> Result<Option<CalendarEvent>> {
        let row = sqlx::query_as::<_, CalendarEventRow>(
            r#"SELECT id, account_id, calendar_id, provider_event_id, title, description,
                      start_at, end_at, location, is_all_day, recurrence_rule, attendees_json, meet_link
               FROM calendar_events WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into_event()))
    }

    /// Upsert a calendar event — matches on provider_event_id if present, otherwise on id.
    pub async fn upsert_event(&self, account_id: &str, event: &CalendarEvent) -> Result<String> {
        let existing_id: Option<String> = if let Some(pid) = &event.provider_event_id {
            sqlx::query_scalar(
                "SELECT id FROM calendar_events WHERE account_id = ? AND calendar_id = ? AND provider_event_id = ?",
            )
            .bind(account_id)
            .bind(&event.calendar_id)
            .bind(pid)
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };

        let id = existing_id.unwrap_or_else(|| {
            if event.id.is_empty() {
                Uuid::new_v4().to_string()
            } else {
                event.id.clone()
            }
        });

        let attendees_json =
            serde_json::to_string(&event.attendees).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            r#"INSERT INTO calendar_events
               (id, account_id, calendar_id, provider_event_id, title, description,
                start_at, end_at, location, is_all_day, recurrence_rule, attendees_json,
                meet_link, synced_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
               ON CONFLICT(id) DO UPDATE SET
                   title          = excluded.title,
                   description    = excluded.description,
                   start_at       = excluded.start_at,
                   end_at         = excluded.end_at,
                   location       = excluded.location,
                   is_all_day     = excluded.is_all_day,
                   recurrence_rule = excluded.recurrence_rule,
                   attendees_json = excluded.attendees_json,
                   meet_link      = excluded.meet_link,
                   synced_at      = datetime('now')"#,
        )
        .bind(&id)
        .bind(account_id)
        .bind(&event.calendar_id)
        .bind(&event.provider_event_id)
        .bind(&event.title)
        .bind(&event.description)
        .bind(event.start_at.to_rfc3339())
        .bind(event.end_at.to_rfc3339())
        .bind(&event.location)
        .bind(event.is_all_day)
        .bind(&event.recurrence_rule)
        .bind(&attendees_json)
        .bind(&event.meet_link)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    /// Look up a locally stored event by its iCalendar UID (stored as the
    /// provider event id of invitation-created events).
    pub async fn find_event_by_uid(&self, account_id: &str, uid: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT id FROM calendar_events WHERE account_id = ? AND calendar_id = 'default' AND provider_event_id = ?",
        )
        .bind(account_id)
        .bind(uid)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Remove the event stored for an iCalendar UID. Returns whether one existed.
    pub async fn delete_event_by_uid(&self, account_id: &str, uid: &str) -> Result<bool> {
        let Some(id) = self.find_event_by_uid(account_id, uid).await? else {
            return Ok(false);
        };
        sqlx::query("DELETE FROM message_calendar_links WHERE event_id = ?")
            .bind(&id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM calendar_events WHERE id = ?")
            .bind(&id)
            .execute(&self.pool)
            .await?;
        Ok(true)
    }

    // ── Link management ───────────────────────────────────────────────────────

    pub async fn list_event_links(&self, event_id: &str) -> Result<Vec<EventLink>> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: String,
            event_id: String,
            message_id: String,
            folder_id: String,
            linked_at: String,
            subject: Option<String>,
            from_name: Option<String>,
            from_email: Option<String>,
            date: Option<String>,
        }

        let rows = sqlx::query_as::<_, Row>(
            r#"SELECT l.id, l.event_id, l.message_id, l.linked_at,
                      m.folder_id, m.subject, m.from_name, m.from_email, m.date
               FROM message_calendar_links l
               JOIN messages m ON m.id = l.message_id
               WHERE l.event_id = ?
               ORDER BY l.linked_at DESC"#,
        )
        .bind(event_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| EventLink {
                id: r.id,
                event_id: r.event_id,
                message_id: r.message_id,
                folder_id: r.folder_id,
                subject: r.subject,
                from_name: r.from_name,
                from_email: r.from_email,
                date: r.date,
                linked_at: r.linked_at,
            })
            .collect())
    }

    pub async fn add_event_link(&self, event_id: &str, message_id: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT OR IGNORE INTO message_calendar_links (id, event_id, message_id) \
             VALUES (?, ?, ?)",
        )
        .bind(&id)
        .bind(event_id)
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn remove_event_link(&self, event_id: &str, message_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM message_calendar_links WHERE event_id = ? AND message_id = ?")
            .bind(event_id)
            .bind(message_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn service() -> CalendarService {
        let (pool, _) = crate::db::create_pool("sqlite::memory:").await.unwrap();
        sqlx::query("INSERT INTO accounts (id, name, email, provider_type, auth_type) VALUES ('acc', 'Me', 'me@example.com', 'imap', 'password')")
            .execute(&pool)
            .await
            .unwrap();
        CalendarService::new(pool)
    }

    fn event(calendar_id: &str, provider_id: &str, title: &str) -> CalendarEvent {
        CalendarEvent {
            id: String::new(),
            calendar_id: calendar_id.into(),
            provider_event_id: Some(provider_id.into()),
            title: title.into(),
            description: None,
            start_at: "2026-11-06T12:00:00Z".parse().unwrap(),
            end_at: "2026-11-06T13:00:00Z".parse().unwrap(),
            location: None,
            is_all_day: false,
            recurrence_rule: None,
            attendees: vec![],
            meet_link: None,
        }
    }

    #[tokio::test]
    async fn resyncing_an_event_in_a_non_default_calendar_updates_it() {
        let service = service().await;
        let first = service
            .upsert_event("acc", &event("primary", "g-1", "Before"))
            .await
            .unwrap();
        let second = service
            .upsert_event("acc", &event("primary", "g-1", "After"))
            .await
            .unwrap();
        assert_eq!(first, second);
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT calendar_id, title FROM calendar_events")
                .fetch_all(&service.pool)
                .await
                .unwrap();
        assert_eq!(rows, vec![("primary".to_string(), "After".to_string())]);
    }

    #[tokio::test]
    async fn the_same_provider_id_in_two_calendars_stays_two_events() {
        let service = service().await;
        let synced = service
            .upsert_event("acc", &event("primary", "shared", "Synced"))
            .await
            .unwrap();
        let local = service
            .upsert_event("acc", &event("default", "shared", "Invitation"))
            .await
            .unwrap();
        assert_ne!(synced, local);
        let again = service
            .upsert_event("acc", &event("primary", "shared", "Synced v2"))
            .await
            .unwrap();
        assert_eq!(again, synced);
        assert_eq!(
            service.find_event_by_uid("acc", "shared").await.unwrap(),
            Some(local)
        );
    }
}
