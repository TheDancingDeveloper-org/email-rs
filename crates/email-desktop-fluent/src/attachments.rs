use crate::prelude::*;
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttachmentAction {
    Open,
    Download,
}

/// Largest image the in-app lightbox will download and decode.
pub(crate) const MAX_PREVIEW_BYTES: i64 = 20 * 1024 * 1024;

/// Largest decoded image the lightbox accepts: per side and in total pixels.
pub(crate) const MAX_PREVIEW_SIDE: u32 = 16_384;
pub(crate) const MAX_PREVIEW_PIXELS: u64 = 64_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttachmentPreview {
    pub(crate) message_id: String,
    pub(crate) attachment_id: String,
    pub(crate) path: PathBuf,
    pub(crate) filename: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SavedAttachment {
    pub(crate) path: PathBuf,
    pub(crate) filename: String,
}

pub(crate) fn start_attachment_open(
    state_entity: &Entity<NativeAppState>,
    message_id: &str,
    attachment: MessageAttachment,
    app_cx: &mut App,
) {
    if attachment_requires_confirmation(&attachment) {
        let confirmed = state_entity.update(app_cx, |state, cx| {
            if state.pending_unsafe_attachment.as_deref() == Some(attachment.id.as_str()) {
                state.pending_unsafe_attachment = None;
                true
            } else {
                state.pending_unsafe_attachment = Some(attachment.id.clone());
                state.attachment_notice = Some(format!(
                    "{} may be unsafe. Click Open again to confirm.",
                    attachment.filename
                ));
                state.attachment_error = None;
                cx.notify();
                false
            }
        });
        if !confirmed {
            return;
        }
    }
    start_attachment_action(
        state_entity,
        message_id,
        attachment,
        AttachmentAction::Open,
        app_cx,
    );
}

/// Raster formats the lightbox can decode. SVG is excluded on purpose: it can
/// reference external resources, so it stays on the confirm-and-open path.
pub(crate) fn attachment_is_previewable(attachment: &MessageAttachment) -> bool {
    matches!(
        attachment.content_type.to_ascii_lowercase().as_str(),
        "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp"
    ) && (0..=MAX_PREVIEW_BYTES).contains(&attachment.size_bytes)
}

pub(crate) fn start_attachment_preview(
    state_entity: &Entity<NativeAppState>,
    message_id: &str,
    attachment: MessageAttachment,
    app_cx: &mut App,
) {
    if !attachment_is_previewable(&attachment) {
        return;
    }
    let message_id = message_id.to_string();
    let Some(base_url) = state_entity.update(app_cx, |state, cx| {
        if state.attachment_busy || state.selected_message.as_deref() != Some(message_id.as_str()) {
            return None;
        }
        state.attachment_busy = true;
        state.attachment_notice = Some(format!("Loading preview of {}...", attachment.filename));
        state.attachment_error = None;
        cx.notify();
        Some(state.api_base_url.clone())
    }) else {
        return;
    };

    let task_message_id = message_id.clone();
    let task_attachment = attachment.clone();
    let task = app_cx.background_executor().spawn(async move {
        let directory = attachment_open_dir(&task_message_id, &task_attachment.id);
        save_preview_to_dir(base_url, &task_message_id, &task_attachment, directory)
    });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                state_entity.update(&mut async_cx, |state, cx| {
                    finish_preview(state, &message_id, &attachment.id, result);
                    cx.notify();
                });
            }
        })
        .detach();
}

/// Apply a finished preview download. The busy flag is always released; the
/// result is only shown if the user is still on the message it was for.
pub(crate) fn finish_preview(
    state: &mut NativeAppState,
    message_id: &str,
    attachment_id: &str,
    result: Result<SavedAttachment>,
) {
    state.attachment_busy = false;
    if state.selected_message.as_deref() != Some(message_id) {
        state.attachment_notice = None;
        return;
    }
    match result {
        Ok(saved) => {
            state.attachment_notice = None;
            state.attachment_preview = Some(AttachmentPreview {
                message_id: message_id.to_string(),
                attachment_id: attachment_id.to_string(),
                path: saved.path,
                filename: saved.filename,
            });
        }
        Err(err) => {
            state.attachment_notice = None;
            state.attachment_error = Some(format!("Attachment preview failed: {err}"));
        }
    }
}

/// The preview to show for the open message, if any.
pub(crate) fn preview_for_message<'a>(
    preview: Option<&'a AttachmentPreview>,
    message_id: &str,
) -> Option<&'a AttachmentPreview> {
    preview.filter(|preview| preview.message_id == message_id)
}

pub(crate) fn attachment_requires_confirmation(attachment: &MessageAttachment) -> bool {
    let content_type = attachment.content_type.to_ascii_lowercase();
    !(content_type == "application/pdf"
        || content_type == "text/plain"
        || content_type.starts_with("image/"))
}

pub(crate) fn start_attachment_download(
    state_entity: &Entity<NativeAppState>,
    message_id: &str,
    attachment: MessageAttachment,
    app_cx: &mut App,
) {
    start_attachment_action(
        state_entity,
        message_id,
        attachment,
        AttachmentAction::Download,
        app_cx,
    );
}

pub(crate) fn start_attachment_action(
    state_entity: &Entity<NativeAppState>,
    message_id: &str,
    attachment: MessageAttachment,
    action: AttachmentAction,
    app_cx: &mut App,
) {
    let message_id = message_id.to_string();
    let Some(base_url) = state_entity.update(app_cx, |state, cx| {
        if state.attachment_busy || state.selected_message.as_deref() != Some(message_id.as_str()) {
            return None;
        }
        state.attachment_busy = true;
        state.attachment_notice = Some(match action {
            AttachmentAction::Open => format!("Opening {}...", attachment.filename),
            AttachmentAction::Download => format!("Saving {}...", attachment.filename),
        });
        state.attachment_error = None;
        state.last_error = None;
        cx.notify();
        Some(state.api_base_url.clone())
    }) else {
        return;
    };

    let task_message_id = message_id.clone();
    let task_attachment = attachment.clone();
    let task = app_cx.background_executor().spawn(async move {
        run_attachment_action(base_url, task_message_id, task_attachment, action)
    });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                state_entity.update(&mut async_cx, |state, cx| {
                    finish_action(state, &message_id, action, result);
                    cx.notify();
                });
            }
        })
        .detach();
}

/// Apply a finished open or download. The busy flag is always released; the
/// result is only shown if the user is still on the message it was for.
pub(crate) fn finish_action(
    state: &mut NativeAppState,
    message_id: &str,
    action: AttachmentAction,
    result: Result<SavedAttachment>,
) {
    state.attachment_busy = false;
    if state.selected_message.as_deref() != Some(message_id) {
        state.attachment_notice = None;
        return;
    }
    match result {
        Ok(saved) => {
            state.attachment_error = None;
            state.last_error = None;
            state.attachment_notice = Some(match action {
                AttachmentAction::Open => {
                    format!("Opened {} from {}", saved.filename, saved.path.display())
                }
                AttachmentAction::Download => {
                    format!("Saved {} to {}", saved.filename, saved.path.display())
                }
            });
        }
        Err(err) => {
            let message = match action {
                AttachmentAction::Open => {
                    format!("Attachment open failed: {err}")
                }
                AttachmentAction::Download => {
                    format!("Attachment download failed: {err}")
                }
            };
            state.attachment_notice = None;
            state.attachment_error = Some(message.clone());
            state.last_error = Some(message);
        }
    }
}

pub(crate) fn run_attachment_action(
    base_url: String,
    message_id: String,
    attachment: MessageAttachment,
    action: AttachmentAction,
) -> Result<SavedAttachment> {
    let directory = match action {
        AttachmentAction::Open => attachment_open_dir(&message_id, &attachment.id),
        AttachmentAction::Download => attachment_download_dir(),
    };
    let saved = save_attachment_to_dir(base_url, &message_id, &attachment, directory)?;
    if action == AttachmentAction::Open {
        open_path_with_system(&saved.path)?;
    }
    Ok(saved)
}

pub(crate) fn save_attachment_to_dir(
    base_url: String,
    message_id: &str,
    attachment: &MessageAttachment,
    directory: PathBuf,
) -> Result<SavedAttachment> {
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("create {}", directory.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("secure {}", directory.display()))?;
    }
    let filename = safe_attachment_filename(&attachment.filename);
    let path = unique_attachment_path(&directory, &filename);
    let bytes = download_attachment_bytes(base_url, message_id, &attachment.id)?;
    write_private_attachment(&path, &bytes)?;
    Ok(SavedAttachment { path, filename })
}

/// Download an image for the lightbox and write it only if the bytes really
/// are a raster image within the size and dimension caps.
pub(crate) fn save_preview_to_dir(
    base_url: String,
    message_id: &str,
    attachment: &MessageAttachment,
    directory: PathBuf,
) -> Result<SavedAttachment> {
    let bytes = download_attachment_bytes(base_url, message_id, &attachment.id)?;
    validate_preview_bytes(&bytes)?;
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("create {}", directory.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("secure {}", directory.display()))?;
    }
    let filename = safe_attachment_filename(&attachment.filename);
    let path = unique_attachment_path(&directory, &filename);
    write_private_attachment(&path, &bytes)?;
    Ok(SavedAttachment { path, filename })
}

/// Check downloaded bytes, ignoring the sender's declared metadata: actual
/// length, a PNG/JPEG/GIF/WebP signature, and header dimensions.
pub(crate) fn validate_preview_bytes(bytes: &[u8]) -> Result<()> {
    if bytes.len() as u64 > MAX_PREVIEW_BYTES as u64 {
        anyhow::bail!(
            "image is larger than {} MiB",
            MAX_PREVIEW_BYTES / (1024 * 1024)
        );
    }
    let Some((width, height)) = image_header_dimensions(bytes) else {
        anyhow::bail!("not a PNG, JPEG, GIF or WebP image");
    };
    if width == 0
        || height == 0
        || width > MAX_PREVIEW_SIDE
        || height > MAX_PREVIEW_SIDE
        || u64::from(width) * u64::from(height) > MAX_PREVIEW_PIXELS
    {
        anyhow::bail!("image dimensions {width}x{height} are too large to preview");
    }
    Ok(())
}

/// Pixel dimensions from the header of a PNG, JPEG, GIF or WebP; `None` when
/// the signature is not one of those or the header is truncated.
pub(crate) fn image_header_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let be32 = |at: usize| {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
    };
    let le16 = |at: usize| {
        bytes
            .get(at..at + 2)
            .map(|b| u32::from(u16::from_le_bytes(b.try_into().unwrap())))
    };
    let le24 = |at: usize| {
        bytes
            .get(at..at + 3)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], 0]))
    };
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        if bytes.get(12..16)? != b"IHDR" {
            return None;
        }
        return Some((be32(16)?, be32(20)?));
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some((le16(6)?, le16(8)?));
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        let mut at = 2;
        while at + 4 <= bytes.len() {
            if bytes[at] != 0xFF {
                return None;
            }
            let marker = bytes[at + 1];
            if marker == 0xFF {
                at += 1;
                continue;
            }
            if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                let height = u32::from(u16::from_be_bytes(
                    bytes.get(at + 5..at + 7)?.try_into().ok()?,
                ));
                let width = u32::from(u16::from_be_bytes(
                    bytes.get(at + 7..at + 9)?.try_into().ok()?,
                ));
                return Some((width, height));
            }
            let len = usize::from(u16::from_be_bytes(
                bytes.get(at + 2..at + 4)?.try_into().ok()?,
            ));
            if len < 2 {
                return None;
            }
            at += 2 + len;
        }
        return None;
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12)? == b"WEBP" {
        return match bytes.get(12..16)? {
            b"VP8 " => {
                if bytes.get(23..26)? != [0x9d, 0x01, 0x2a] {
                    return None;
                }
                Some((le16(26)? & 0x3fff, le16(28)? & 0x3fff))
            }
            b"VP8L" => {
                if *bytes.get(20)? != 0x2f {
                    return None;
                }
                let bits = u32::from_le_bytes(bytes.get(21..25)?.try_into().ok()?);
                Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
            }
            b"VP8X" => Some((le24(24)? + 1, le24(27)? + 1)),
            _ => None,
        };
    }
    None
}

pub(crate) fn write_private_attachment(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    std::io::Write::write_all(&mut file, bytes).with_context(|| format!("write {}", path.display()))
}

pub(crate) fn download_attachment_bytes(
    base_url: String,
    message_id: &str,
    attachment_id: &str,
) -> Result<Vec<u8>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .download_attachment(message_id, attachment_id)
            .await
            .with_context(|| format!("download attachment {attachment_id}"))
    })
}

pub(crate) fn attachment_open_dir(message_id: &str, attachment_id: &str) -> PathBuf {
    std::env::temp_dir()
        .join("email-rs-attachments")
        .join(safe_attachment_filename(message_id))
        .join(safe_attachment_filename(attachment_id))
}

pub(crate) fn attachment_download_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Downloads"))
        .filter(|path| path.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

pub(crate) fn unique_attachment_path(directory: &Path, filename: &str) -> PathBuf {
    let initial = directory.join(filename);
    if !initial.exists() {
        return initial;
    }

    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("attachment");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    for index in 1..1000 {
        let candidate = match extension {
            Some(extension) => directory.join(format!("{stem} ({index}).{extension}")),
            None => directory.join(format!("{stem} ({index})")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }

    directory.join(format!("{stem} ({})", Utc::now().timestamp_millis()))
}

pub(crate) fn safe_attachment_filename(filename: &str) -> String {
    let safe = filename
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect::<String>();
    let safe = safe.trim_matches(|ch| ch == '.' || ch == ' ').trim();
    if safe.is_empty() {
        "attachment".to_string()
    } else {
        safe.to_string()
    }
}

pub(crate) fn attachment_size_label(size_bytes: i64) -> String {
    let bytes = size_bytes.max(0) as f64;
    if bytes < 1024.0 {
        format!("{} B", bytes as i64)
    } else if bytes < 1024.0 * 1024.0 {
        format!("{:.1} KB", bytes / 1024.0)
    } else {
        format!("{:.1} MB", bytes / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;

    fn attachment(content_type: &str, size_bytes: i64) -> MessageAttachment {
        MessageAttachment {
            id: "a1".into(),
            filename: "pic".into(),
            content_type: content_type.into(),
            size_bytes,
            message_id: "m1".into(),
            dangerous_or_double_extension: false,
        }
    }

    #[test]
    fn previews_only_raster_images_within_the_size_cap() {
        assert!(attachment_is_previewable(&attachment("image/PNG", 10)));
        assert!(attachment_is_previewable(&attachment(
            "image/jpeg",
            MAX_PREVIEW_BYTES
        )));
        assert!(!attachment_is_previewable(&attachment(
            "image/jpeg",
            MAX_PREVIEW_BYTES + 1
        )));
        assert!(!attachment_is_previewable(&attachment("image/svg+xml", 10)));
        assert!(!attachment_is_previewable(&attachment(
            "application/pdf",
            10
        )));
        assert!(!attachment_is_previewable(&attachment("image/png", -1)));
    }

    #[test]
    fn preview_is_scoped_to_its_message() {
        let preview = AttachmentPreview {
            message_id: "m1".into(),
            attachment_id: "a1".into(),
            path: PathBuf::from("/tmp/x.png"),
            filename: "x.png".into(),
        };
        assert!(preview_for_message(Some(&preview), "m1").is_some());
        assert!(preview_for_message(Some(&preview), "m2").is_none());
        assert!(preview_for_message(None, "m1").is_none());
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes
    }

    #[test]
    fn mislabelled_svg_is_refused() {
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><image href=\"http://x/\"/></svg>";
        assert!(validate_preview_bytes(svg).is_err());
        assert!(validate_preview_bytes(&png(10, 10)).is_ok());
    }

    #[test]
    fn oversize_body_is_refused_whatever_was_declared() {
        let mut body = png(10, 10);
        body.resize(MAX_PREVIEW_BYTES as usize + 1, 0);
        assert!(validate_preview_bytes(&body).is_err());
    }

    #[test]
    fn over_dimension_header_is_refused() {
        assert!(validate_preview_bytes(&png(MAX_PREVIEW_SIDE + 1, 1)).is_err());
        assert!(validate_preview_bytes(&png(10_000, 10_000)).is_err());
        assert!(validate_preview_bytes(&png(8_000, 8_000)).is_ok());
        assert!(validate_preview_bytes(&png(0, 5)).is_err());
    }

    #[test]
    fn header_dimensions_for_each_format() {
        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&[3, 0, 4, 0]);
        assert_eq!(image_header_dimensions(&gif), Some((3, 4)));
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xC0, 0, 11, 8, 0, 7, 0, 6, 1, 0, 0, 0,
        ];
        assert_eq!(image_header_dimensions(&jpeg), Some((6, 7)));
        let mut vp8x = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0\0\0\0\0".to_vec();
        vp8x.extend_from_slice(&[4, 0, 0, 2, 0, 0]);
        assert_eq!(image_header_dimensions(&vp8x), Some((5, 3)));
        let mut vp8l = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2f".to_vec();
        vp8l.extend_from_slice(&((9u32) | (19 << 14)).to_le_bytes());
        assert_eq!(image_header_dimensions(&vp8l), Some((10, 20)));
        assert_eq!(image_header_dimensions(&png(1, 2)[..20]), None);
    }

    #[test]
    fn switching_message_mid_download_releases_busy() {
        let mut state = NativeAppState {
            selected_message: Some("m2".into()),
            attachment_busy: true,
            ..NativeAppState::default()
        };
        let saved = SavedAttachment {
            path: PathBuf::from("/tmp/x.png"),
            filename: "x.png".into(),
        };
        finish_preview(&mut state, "m1", "a1", Ok(saved.clone()));
        assert!(!state.attachment_busy);
        assert!(state.attachment_preview.is_none());

        state.attachment_busy = true;
        finish_action(&mut state, "m1", AttachmentAction::Download, Ok(saved));
        assert!(!state.attachment_busy);
        assert!(state.attachment_notice.is_none());
    }
}
