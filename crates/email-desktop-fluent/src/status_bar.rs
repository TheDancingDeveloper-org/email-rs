use crate::prelude::*;
use crate::*;

#[derive(Clone)]
pub(crate) struct StatusBar {
    pub(crate) state: Entity<NativeAppState>,
}

impl StatusBar {
    pub(crate) fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let typography = theme.typography;
        let status = state
            .last_error
            .clone()
            .unwrap_or_else(|| "Axum in-process server ready".to_string());
        let unread = total_unread_count(&state);
        let body_status = if state.selected_message_resource.is_loading() {
            "body loading".to_string()
        } else if state.selected_message_resource.is_error() {
            "body error".to_string()
        } else if state.selected_message_resource.is_ready() {
            "body ready".to_string()
        } else {
            "body idle".to_string()
        };

        div()
            .h(px(28.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .border_t_1()
            .border_color(colors.stroke_neutral_subtle)
            .bg(colors.panel_bg)
            .text_size(px(typography.caption.size))
            .text_color(if state.last_error.is_some() {
                colors.status_error
            } else {
                colors.on_subtle
            })
            .child(format!(
                "{} | {} | {} | {} | density {} | {:?}",
                status,
                resource_summary(&state.messages_resource, "messages"),
                body_status,
                unread_status_text(unread),
                state.density.level,
                state.theme
            ))
    }
}

pub(crate) fn resource_summary<T>(resource: &AsyncResource<Vec<T>, String>, label: &str) -> String {
    let count = resource.value().map(Vec::len).unwrap_or_default();
    if resource.is_loading() {
        format!("{label} loading ({count} cached)")
    } else if let Some(error) = resource.error() {
        if count > 0 {
            format!("{label} error ({count} cached): {error}")
        } else {
            format!("{label} error: {error}")
        }
    } else if resource.is_ready() {
        format!("{count} {label} ready")
    } else {
        format!("{label} idle")
    }
}

pub(crate) fn total_unread_count(state: &NativeAppState) -> i64 {
    state
        .folders_by_account
        .values()
        .flat_map(|folders| folders.iter())
        .map(|folder| folder.unread_count.max(0))
        .sum()
}

pub(crate) fn native_window_title(state: &NativeAppState) -> String {
    let unread = total_unread_count(state);
    if unread > 0 {
        format!("({unread}) {APP_TITLE}")
    } else {
        APP_TITLE.to_string()
    }
}

pub(crate) fn native_window_status(state: &NativeAppState) -> String {
    if initial_accounts_loading(state) {
        return "Loading accounts...".to_string();
    }
    unread_status_text(total_unread_count(state))
}

pub(crate) fn unread_status_text(unread: i64) -> String {
    match unread {
        0 => "No unread messages".to_string(),
        1 => "1 unread message".to_string(),
        unread => format!("{unread} unread messages"),
    }
}

pub(crate) fn sync_update_requests_refresh(update: &NativeSyncUpdate) -> bool {
    matches!(
        update,
        NativeSyncUpdate::Event(SyncEvent::SyncComplete { .. })
    )
}

pub(crate) fn apply_sync_update(status: &mut NativeSyncStatus, update: NativeSyncUpdate) {
    match update {
        NativeSyncUpdate::Connecting => {
            status.connected = false;
            status.last_error = None;
            status.last_message = Some("Connecting to sync events".to_string());
            status.completed_at = None;
        }
        NativeSyncUpdate::Connected => {
            status.connected = true;
            status.last_error = None;
            status.last_message = Some("Waiting for sync activity".to_string());
            status.completed_at = None;
        }
        NativeSyncUpdate::Event(SyncEvent::SyncStart {
            account_id,
            folder_count,
        }) => {
            status.connected = true;
            status.syncing = true;
            status.account_id = Some(account_id);
            status.folder = None;
            status.done = 0;
            status.total = folder_count.unwrap_or_default();
            status.last_error = None;
            status.last_message = Some("Sync started".to_string());
            status.completed_at = None;
        }
        NativeSyncUpdate::Event(SyncEvent::SyncFolderDone {
            account_id,
            folder,
            done,
            total,
        }) => {
            status.connected = true;
            status.syncing = true;
            status.account_id = Some(account_id);
            status.folder = folder;
            status.done = done.unwrap_or(status.done);
            status.total = total.unwrap_or(status.total);
            status.last_error = None;
            status.last_message = Some("Syncing folders".to_string());
            status.completed_at = None;
        }
        NativeSyncUpdate::Event(SyncEvent::SyncComplete { account_id }) => {
            status.connected = true;
            status.syncing = false;
            status.account_id = Some(account_id);
            status.folder = None;
            status.done = status.total.max(status.done);
            status.last_error = None;
            status.last_message = Some("Sync complete".to_string());
            status.completed_at = Some(Instant::now());
        }
        NativeSyncUpdate::Error(message) => {
            status.connected = false;
            status.syncing = false;
            status.last_error = Some(message);
            status.completed_at = None;
        }
    }
}

pub(crate) fn sync_status_visible(status: &NativeSyncStatus) -> bool {
    status.syncing || status.last_error.is_some() || status.completed_at.is_some()
}

pub(crate) fn expire_sync_completion(status: &mut NativeSyncStatus, now: Instant) -> bool {
    if status.syncing || status.last_error.is_some() {
        return false;
    }
    let Some(completed_at) = status.completed_at else {
        return false;
    };
    if now.saturating_duration_since(completed_at) < SYNC_COMPLETE_VISIBLE_FOR {
        return false;
    }

    status.completed_at = None;
    status.last_message = None;
    status.folder = None;
    status.done = 0;
    status.total = 0;
    true
}

pub(crate) fn sync_status_title(status: &NativeSyncStatus) -> String {
    if let Some(error) = status.last_error.as_deref() {
        return format!("Sync events offline: {error}");
    }
    if status.syncing {
        if status.total > 0 {
            return format!("Syncing {} of {}", status.done.max(0), status.total);
        }
        return "Syncing".to_string();
    }
    status
        .last_message
        .clone()
        .unwrap_or_else(|| "Sync events idle".to_string())
}

pub(crate) fn sync_status_detail(status: &NativeSyncStatus) -> String {
    if status.syncing {
        return status
            .folder
            .clone()
            .filter(|folder| !folder.trim().is_empty())
            .unwrap_or_else(|| "Preparing folders".to_string());
    }
    if status.connected {
        "Live sync listener connected".to_string()
    } else {
        "Live sync listener not connected".to_string()
    }
}

pub(crate) fn sync_progress_ratio(status: &NativeSyncStatus) -> f32 {
    if status.total <= 0 {
        return 0.0;
    }
    (status.done.max(0) as f32 / status.total as f32).clamp(0.0, 1.0)
}

pub(crate) fn sync_window_title(state: &NativeAppState, cx: &mut App) {
    WindowTitle::set_title(cx, native_window_title(state));
    WindowTitle::set_status(cx, native_window_status(state));
}
