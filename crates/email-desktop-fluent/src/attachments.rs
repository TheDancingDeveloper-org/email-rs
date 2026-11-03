use crate::prelude::*;
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttachmentAction {
    Open,
    Download,
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
                    if state.selected_message.as_deref() != Some(message_id.as_str()) {
                        return;
                    }
                    state.attachment_busy = false;
                    match result {
                        Ok(saved) => {
                            state.attachment_error = None;
                            state.last_error = None;
                            state.attachment_notice = Some(match action {
                                AttachmentAction::Open => format!(
                                    "Opened {} from {}",
                                    saved.filename,
                                    saved.path.display()
                                ),
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
                    cx.notify();
                });
            }
        })
        .detach();
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
