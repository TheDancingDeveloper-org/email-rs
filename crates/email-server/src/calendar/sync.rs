use chrono::{Duration, Utc};
use sqlx::SqlitePool;
use tracing::{info, warn};

use crate::auth::oauth2::StoredToken;
use crate::calendar::CalendarService;
use crate::error::{AppError, Result};
use crate::providers::google_calendar::GoogleCalendarProvider;
use crate::providers::CalendarProvider;

// ── Sync a single Gmail account's calendar ────────────────────────────────────

pub async fn sync_gmail_calendar(pool: &SqlitePool, account_id: &str) -> Result<()> {
    let account = crate::providers::factory::load_account_config(pool, account_id).await?;
    let token_json = account
        .oauth_token_json
        .as_deref()
        .ok_or_else(|| AppError::Auth("account has no OAuth token".into()))?;
    let stored: StoredToken = serde_json::from_str(token_json)
        .map_err(|e| AppError::Auth(format!("parse token: {}", e)))?;

    let provider = GoogleCalendarProvider::new(
        account_id.to_string(),
        account.email,
        stored.access_token.clone(),
    );

    let now = Utc::now();
    let from = now - Duration::days(7);
    let to = now + Duration::days(60);

    let calendars = match provider.list_calendars().await {
        Ok(c) => c,
        Err(e) => {
            warn!("calendar list_calendars failed for {}: {}", account_id, e);
            return Err(e);
        }
    };

    let service = CalendarService::new(pool.clone());
    let mut synced = 0usize;

    for calendar in &calendars {
        let events = match provider.list_events(&calendar.id, from, to).await {
            Ok(e) => e,
            Err(e) => {
                warn!(
                    "list_events failed for calendar {} (account {}): {}",
                    calendar.id, account_id, e
                );
                continue;
            }
        };

        for event in &events {
            if let Err(e) = service.upsert_event(account_id, event).await {
                warn!("upsert_event failed for {}: {}", event.id, e);
            } else {
                synced += 1;
            }
        }
    }

    info!(
        "calendar sync complete for account {}: {} events across {} calendars",
        account_id,
        synced,
        calendars.len()
    );

    Ok(())
}
