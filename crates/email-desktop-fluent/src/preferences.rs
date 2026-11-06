use crate::prelude::*;
use crate::*;

pub(crate) fn load_native_preferences() -> NativePreferences {
    let path = native_settings_path();
    match std::fs::read_to_string(&path) {
        Ok(raw) => parse_native_preferences(&raw),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => NativePreferences::default(),
        Err(err) => {
            tracing::warn!("failed to read native FluentGUI settings from {path:?}: {err}");
            NativePreferences::default()
        }
    }
}

pub(crate) fn save_native_preferences(preferences: &NativePreferences) -> Result<()> {
    write_native_preferences(&native_settings_path(), preferences)
}

pub(crate) fn native_auth_path() -> PathBuf {
    native_config_dir().join(NATIVE_AUTH_FILE)
}

pub(crate) fn load_native_auth_token() -> Option<String> {
    std::fs::read_to_string(native_auth_path())
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn save_native_auth_token(token: &str) -> Result<()> {
    let path = native_auth_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&path);
    write_private_attachment(&path, token.as_bytes())
}

pub(crate) fn clear_native_auth_token() {
    let _ = std::fs::remove_file(native_auth_path());
    set_default_bearer_token(None);
}

pub(crate) fn write_native_preferences(path: &Path, preferences: &NativePreferences) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create native settings directory {parent:?}"))?;
    }
    std::fs::write(path, serialize_native_preferences(preferences))
        .with_context(|| format!("write native settings {path:?}"))
}

pub(crate) fn parse_native_preferences(raw: &str) -> NativePreferences {
    let mut preferences = NativePreferences::default();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "theme" => {
                if let Some(theme) = ThemeMode::parse(value) {
                    preferences.theme = theme;
                }
            }
            "density" => {
                if let Ok(level) = value.trim().parse::<u8>() {
                    preferences.density_level = level.min(8);
                }
            }
            "email_sidebar_w" | "sidebar_width" => {
                if let Some(width) = parse_layout_width(value, MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH)
                {
                    preferences.layout.sidebar_width = width;
                }
            }
            "email_msglist_w" | "message_list_width" => {
                if let Some(width) =
                    parse_layout_width(value, MIN_MESSAGE_LIST_WIDTH, MAX_MESSAGE_LIST_WIDTH)
                {
                    preferences.layout.message_list_width = width;
                }
            }
            "remote_image_allowlist" => {
                preferences.remote_image_allowlist = value
                    .split(',')
                    .map(str::trim)
                    .filter(|sender| !sender.is_empty())
                    .map(str::to_ascii_lowercase)
                    .collect();
            }
            "undo_window_secs" => {
                if let Ok(secs) = value.trim().parse::<u8>() {
                    preferences.undo_window_secs = clamp_undo_window(secs);
                }
            }
            "mark_read_delay_secs" => {
                if let Ok(secs) = value.trim().parse::<u8>() {
                    preferences.mark_read_delay_secs = clamp_mark_read_delay(secs);
                }
            }
            "reply_all_default" => preferences.reply_all_default = parse_bool(value),
            "conversation_view" => preferences.conversation_view = parse_bool(value),
            "notifications_enabled" => preferences.notifications_enabled = parse_bool(value),
            other => {
                if let Some(id) = other.strip_prefix("key.") {
                    let keystroke = value.trim();
                    if native_key_action_by_id(id).is_some() && !keystroke.is_empty() {
                        preferences
                            .key_overrides
                            .insert(id.to_string(), keystroke.to_ascii_lowercase());
                    }
                }
            }
        }
    }
    preferences
}

pub(crate) fn serialize_native_preferences(preferences: &NativePreferences) -> String {
    let mut out = format!(
        "theme={}\ndensity={}\nemail_sidebar_w={}\nemail_msglist_w={}\nremote_image_allowlist={}\nundo_window_secs={}\nmark_read_delay_secs={}\nreply_all_default={}\nconversation_view={}\nnotifications_enabled={}\n",
        preferences.theme.as_str(),
        preferences.density_level.min(8),
        preferences.layout.sidebar_width,
        preferences.layout.message_list_width,
        preferences
            .remote_image_allowlist
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(","),
        clamp_undo_window(preferences.undo_window_secs),
        clamp_mark_read_delay(preferences.mark_read_delay_secs),
        preferences.reply_all_default,
        preferences.conversation_view,
        preferences.notifications_enabled
    );
    for (id, keystroke) in &preferences.key_overrides {
        out.push_str(&format!("key.{id}={keystroke}\n"));
    }
    out
}

pub(crate) const DEFAULT_UNDO_WINDOW_SECS: u8 = 5;
pub(crate) const MIN_UNDO_WINDOW_SECS: u8 = 1;
pub(crate) const MAX_UNDO_WINDOW_SECS: u8 = 30;
pub(crate) const DEFAULT_MARK_READ_DELAY_SECS: u8 = 0;
pub(crate) const MAX_MARK_READ_DELAY_SECS: u8 = 30;
pub(crate) const DEFAULT_REPLY_ALL_DEFAULT: bool = false;
pub(crate) const DEFAULT_CONVERSATION_VIEW: bool = true;
pub(crate) const DEFAULT_NOTIFICATIONS_ENABLED: bool = true;

pub(crate) fn clamp_mark_read_delay(secs: u8) -> u8 {
    secs.min(MAX_MARK_READ_DELAY_SECS)
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

pub(crate) fn clamp_undo_window(secs: u8) -> u8 {
    secs.clamp(MIN_UNDO_WINDOW_SECS, MAX_UNDO_WINDOW_SECS)
}

/// Step the undo window by `delta` seconds, persist it and return the result.
pub(crate) fn adjust_undo_window(
    state_entity: &Entity<NativeAppState>,
    delta: i8,
    app_cx: &mut App,
) {
    let preferences = state_entity.update(app_cx, |state, cx| {
        let next = clamp_undo_window(
            (state.undo_window_secs as i16 + delta as i16).clamp(0, u8::MAX as i16) as u8,
        );
        if next == state.undo_window_secs {
            return None;
        }
        state.undo_window_secs = next;
        cx.notify();
        Some(NativePreferences::from_state(state))
    });
    if let Some(preferences) = preferences {
        if let Err(err) = save_native_preferences(&preferences) {
            tracing::warn!("failed to save undo window: {err:#}");
        }
    }
}

pub(crate) fn update_native_preferences(
    state_entity: &Entity<NativeAppState>,
    app_cx: &mut App,
    update: impl FnOnce(&mut NativeAppState),
) {
    let preferences = state_entity.update(app_cx, |state, cx| {
        update(state);
        cx.notify();
        NativePreferences::from_state(state)
    });
    if let Err(err) = save_native_preferences(&preferences) {
        tracing::warn!("failed to save native preferences: {err:#}");
    }
}

pub(crate) fn parse_layout_width(value: &str, min: u16, max: u16) -> Option<u16> {
    let width = value.trim().parse::<f32>().ok()?;
    width
        .is_finite()
        .then(|| clamp_layout_width(width, min, max))
}

pub(crate) fn clamp_layout_width(width: f32, min: u16, max: u16) -> u16 {
    let width = if width.is_finite() { width } else { min as f32 };
    width.round().clamp(min as f32, max as f32) as u16
}

pub(crate) fn message_list_ratio_for_width(
    width: u16,
    window_width: f32,
    sidebar_width: f32,
) -> f32 {
    let content_width = mail_content_width(window_width, sidebar_width);
    (width as f32 / content_width).clamp(0.1, 0.9)
}

pub(crate) fn message_list_width_for_ratio(
    ratio: f32,
    window_width: f32,
    sidebar_width: f32,
) -> u16 {
    let ratio = if ratio.is_finite() { ratio } else { 0.5 };
    clamp_layout_width(
        ratio.clamp(0.1, 0.9) * mail_content_width(window_width, sidebar_width),
        MIN_MESSAGE_LIST_WIDTH,
        MAX_MESSAGE_LIST_WIDTH,
    )
}

pub(crate) fn mail_content_width(window_width: f32, sidebar_width: f32) -> f32 {
    let window_width = if window_width.is_finite() {
        window_width
    } else {
        DEFAULT_WINDOW_WIDTH
    };
    let sidebar_width = if sidebar_width.is_finite() {
        sidebar_width
    } else {
        DEFAULT_SIDEBAR_WIDTH as f32
    };
    (window_width - sidebar_width).max(1.0)
}

pub(crate) fn native_settings_path() -> PathBuf {
    if let Ok(path) = std::env::var(NATIVE_SETTINGS_ENV) {
        return PathBuf::from(path);
    }

    native_config_dir().join(NATIVE_SETTINGS_FILE)
}

pub(crate) fn native_config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("LOCALAPPDATA")
            .or_else(|_| std::env::var("APPDATA"))
            .unwrap_or_else(|_| std::env::var("TEMP").unwrap_or_else(|_| ".".to_string()));
        return PathBuf::from(base).join("email-rs");
    }

    #[cfg(not(target_os = "windows"))]
    {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("HOME")
                    .map(|home| PathBuf::from(home).join(".config"))
                    .unwrap_or_else(|_| PathBuf::from("/tmp"))
            });
        base.join("email-rs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_width_is_clamped_and_rounded() {
        assert_eq!(clamp_layout_width(10.0, 80, 700), 80);
        assert_eq!(clamp_layout_width(900.0, 80, 700), 700);
        assert_eq!(clamp_layout_width(280.4, 80, 700), 280);
        assert_eq!(clamp_layout_width(f32::NAN, 80, 700), 80);
    }

    #[test]
    fn parse_layout_width_rejects_garbage() {
        assert_eq!(parse_layout_width(" 300 ", 80, 700), Some(300));
        assert_eq!(parse_layout_width("1000", 80, 700), Some(700));
        assert_eq!(parse_layout_width("wide", 80, 700), None);
        assert_eq!(parse_layout_width("inf", 80, 700), None);
    }
}
