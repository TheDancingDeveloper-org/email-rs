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
            _ => {}
        }
    }
    preferences
}

pub(crate) fn serialize_native_preferences(preferences: &NativePreferences) -> String {
    format!(
        "theme={}\ndensity={}\nemail_sidebar_w={}\nemail_msglist_w={}\n",
        preferences.theme.as_str(),
        preferences.density_level.min(8),
        preferences.layout.sidebar_width,
        preferences.layout.message_list_width
    )
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
