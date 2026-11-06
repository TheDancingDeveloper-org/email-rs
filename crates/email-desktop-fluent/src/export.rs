//! Message export and system print helpers for the native reader.
use crate::prelude::*;
use crate::*;

pub(crate) fn message_source(message: &FullMessage) -> String {
    if let Some(raw_headers) = message
        .body
        .as_ref()
        .and_then(|body| body.raw_headers.as_deref())
    {
        let mut source = raw_headers.replace("\r\n", "\n").replace('\r', "\n");
        source = source.trim_end_matches('\n').to_string();
        source.push_str("\r\n\r\n");
        source.push_str(&message_body_text(message).replace('\n', "\r\n"));
        return source;
    }

    let mut source = String::new();
    header(
        &mut source,
        "From",
        message.from_email.as_deref().unwrap_or("unknown"),
    );
    header(
        &mut source,
        "To",
        message.to.as_deref().unwrap_or("unknown"),
    );
    header(
        &mut source,
        "Subject",
        message.subject.as_deref().unwrap_or("(no subject)"),
    );
    header(
        &mut source,
        "Date",
        message.date.as_deref().unwrap_or("unknown"),
    );
    header(
        &mut source,
        "Message-ID",
        message.message_id.as_deref().unwrap_or(&message.id),
    );
    source.push_str("MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n");
    source.push_str(&message_body_text(message).replace('\n', "\r\n"));
    source
}

fn header(source: &mut String, name: &str, value: &str) {
    source.push_str(name);
    source.push_str(": ");
    // Header values are untrusted message data; folding controls must never
    // be able to inject a second header.
    source.push_str(&value.replace(['\r', '\n'], " "));
    source.push_str("\r\n");
}

pub(crate) fn message_body_text(message: &FullMessage) -> String {
    message
        .body
        .as_ref()
        .and_then(|body| body.text_body.clone())
        .or_else(|| {
            message
                .body
                .as_ref()
                .and_then(|body| body.clean_body.clone())
        })
        .or_else(|| {
            message.body.as_ref().and_then(|body| {
                body.html_body
                    .as_ref()
                    .map(|html| RichDocument::from_html_fragment_lossy(html).to_plain_text())
            })
        })
        .unwrap_or_default()
}

pub(crate) fn write_eml(message: &FullMessage, path: &Path) -> Result<()> {
    use std::io::Write;
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if !metadata.file_type().is_file() {
            anyhow::bail!("refusing to write non-regular file {}", path.display());
        }
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).truncate(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("create {} without following a symlink", path.display()))?;
    file.write_all(message_source(message).as_bytes())
        .with_context(|| format!("write {}", path.display()))
}

pub(crate) fn save_eml(message: &FullMessage) -> Result<Option<PathBuf>> {
    let Some(path) = rfd::FileDialog::new()
        .set_title("Save message")
        .set_file_name(format!("{}.eml", safe_file_stem(&message.id)))
        .add_filter("Email message", &["eml"])
        .save_file()
    else {
        return Ok(None);
    };
    write_eml(message, &path)?;
    Ok(Some(path))
}

fn safe_file_stem(id: &str) -> String {
    let mut stem: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if stem.is_empty() {
        stem.push_str("message");
    }
    stem.chars().take(80).collect()
}

/// Create a private, unique file without following a pre-existing symlink.
fn unique_file(prefix: &str, extension: &str, directory: &Path) -> Result<PathBuf> {
    let prefix = format!("{prefix}-");
    let suffix = format!(".{extension}");
    let mut builder = tempfile::Builder::new();
    builder.prefix(&prefix).suffix(&suffix);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o600));
    }
    let file = builder
        .tempfile_in(directory)
        .with_context(|| format!("create private export in {}", directory.display()))?;
    let (_file, path) = file.keep().map_err(|error| anyhow!(error.error))?;
    Ok(path)
}

/// Build the printable page. HTML bodies go through the same allowlist
/// sanitizer the server applies for the reader (no scripts, event handlers or
/// remote fetches), and a CSP blocks anything that slips past it.
pub(crate) fn print_html(message: &FullMessage) -> String {
    let html = message
        .body
        .as_ref()
        .and_then(|body| body.html_body.as_deref())
        .map(email_server::sanitize::sanitize_html)
        .unwrap_or_else(|| {
            message_body_text(message)
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('\n', "<br>")
        });
    format!(
        "<!doctype html><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'\">\
         <main>{html}</main>"
    )
}

pub(crate) fn print_message(message: &FullMessage) -> Result<PathBuf> {
    let path = unique_file(
        &format!("email-rs-{}", safe_file_stem(&message.id)),
        "html",
        &std::env::temp_dir(),
    )?;
    std::fs::write(&path, print_html(message))?;
    open_path_with_system(&path)?;
    Ok(path)
}

pub(crate) fn show_source_dialog(message: &FullMessage, cx: &mut App) {
    let body = div()
        .id("message-source")
        .font_family("monospace")
        .text_size(px(12.0))
        .max_h(px(560.0))
        .overflow_y_scroll()
        .child(message_source(message));
    Dialog::new("Message source")
        .body(body)
        .action(DialogAction::new("Close", |_, _, cx| ModalStack::pop(cx)))
        .show(ModalSize::Fraction(0.85, 0.8), cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use email_types::MessageBody;

    #[test]
    fn print_html_strips_scripts_and_event_handlers() {
        let message = FullMessage {
            id: "print-xss".into(),
            occurrence_id: "print-xss".into(),
            logical_id: "logical-print-xss".into(),
            account_id: "account".into(),
            folder_id: "inbox".into(),
            uid: 1,
            message_id: None,
            thread_id: None,
            subject: None,
            from_name: None,
            from_email: None,
            to: None,
            date: None,
            is_read: true,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: None,
            body: Some(MessageBody {
                html_body: Some(
                    "<p>keep me</p><script>alert(1)</script>\
                     <img src=\"https://tracker.example/p.png\" onerror=\"alert(2)\">"
                        .into(),
                ),
                ..Default::default()
            }),
            authentication_results: Default::default(),
            from_reply_to_domain_mismatch: false,
            display_name_spoofing: false,
            list_unsubscribe: None,
            attachments: Vec::new(),
        };
        let out = print_html(&message).to_ascii_lowercase();
        assert!(out.contains("keep me"));
        assert!(!out.contains("<script") && !out.contains("alert"));
        assert!(!out.contains("onerror"));
        assert!(!out.contains("tracker.example"));
    }

    #[cfg(unix)]
    #[test]
    fn unique_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = unique_file("perm", "html", dir.path()).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0);
    }

    #[test]
    fn eml_export_round_trip_preserves_headers_and_body() {
        let mut message = FullMessage {
            id: "round-trip".into(),
            occurrence_id: "round-trip".into(),
            logical_id: "logical-round-trip".into(),
            account_id: "account".into(),
            folder_id: "inbox".into(),
            uid: 1,
            message_id: None,
            thread_id: None,
            subject: Some("Hello".into()),
            from_name: None,
            from_email: Some("a@example.test".into()),
            to: Some("b@example.test".into()),
            date: None,
            is_read: true,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: None,
            body: Some(MessageBody {
                text_body: Some("line one\nline two".into()),
                ..Default::default()
            }),
            authentication_results: Default::default(),
            from_reply_to_domain_mismatch: false,
            display_name_spoofing: false,
            list_unsubscribe: None,
            attachments: Vec::new(),
        };
        let path =
            std::env::temp_dir().join(format!("email-rs-round-trip-{}.eml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        write_eml(&message, &path).unwrap();
        let bytes = std::fs::read_to_string(&path).unwrap();
        assert!(bytes.contains("Subject: Hello\r\n"));
        assert!(bytes.contains("line one\r\nline two"));
        message.body = Some(MessageBody {
            text_body: Some(
                bytes
                    .split("\r\n\r\n")
                    .nth(1)
                    .unwrap()
                    .replace("\r\n", "\n"),
            ),
            ..Default::default()
        });
        assert_eq!(message_body_text(&message), "line one\nline two");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn header_values_cannot_inject_headers() {
        let message = FullMessage {
            subject: Some("hello\r\nX-Injected: yes".into()),
            ..test_message()
        };
        let source = message_source(&message);
        assert!(source.contains("Subject: hello  X-Injected: yes\r\n"));
        assert!(!source.contains("\r\nX-Injected: yes\r\n"));
    }

    #[test]
    fn export_uses_safe_unique_name_and_does_not_follow_symlink() {
        let directory = tempfile::tempdir().unwrap();
        let message = FullMessage {
            id: "../evil\r\nname".into(),
            ..test_message()
        };
        let path = unique_file("evil", "eml", directory.path()).unwrap();
        std::fs::remove_file(&path).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/dev/null", &path).unwrap();
        #[cfg(unix)]
        {
            let result = write_eml(&message, &path);
            assert!(
                result.is_err(),
                "an existing symlink must never be followed"
            );
        }
        assert!(safe_file_stem(&message.id)
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')));
    }

    fn test_message() -> FullMessage {
        FullMessage {
            id: "message".into(),
            occurrence_id: "message".into(),
            logical_id: "logical".into(),
            account_id: "account".into(),
            folder_id: "inbox".into(),
            uid: 1,
            message_id: None,
            thread_id: None,
            subject: Some("subject".into()),
            from_name: None,
            from_email: Some("from@example.test".into()),
            to: Some("to@example.test".into()),
            date: None,
            is_read: true,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: None,
            body: None,
            authentication_results: Default::default(),
            from_reply_to_domain_mismatch: false,
            display_name_spoofing: false,
            list_unsubscribe: None,
            attachments: vec![],
        }
    }
}
