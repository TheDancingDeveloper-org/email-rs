//! Message export and system print helpers for the native reader.
use crate::prelude::*;
use crate::*;

pub(crate) fn message_source(message: &FullMessage) -> String {
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

pub(crate) fn open_source(message: &FullMessage) -> Result<PathBuf> {
    let path = unique_file("email-rs", "eml", &std::env::temp_dir())?;
    write_eml(message, &path)?;
    open_path_with_system(&path)?;
    Ok(path)
}

pub(crate) fn save_eml(message: &FullMessage) -> Result<PathBuf> {
    let directory = std::env::var_os("EMAIL_RS_DOWNLOADS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join("Downloads"))
                .unwrap_or_else(std::env::temp_dir)
        });
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("create export directory {}", directory.display()))?;
    let path = unique_file(&safe_file_stem(&message.id), "eml", &directory)?;
    write_eml(message, &path)?;
    Ok(path)
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
    let file = tempfile::Builder::new()
        .prefix(&format!("{prefix}-"))
        .suffix(&format!(".{extension}"))
        .tempfile_in(directory)
        .with_context(|| format!("create private export in {}", directory.display()))?;
    let (_file, path) = file.keep().map_err(|error| anyhow!(error.error))?;
    Ok(path)
}

pub(crate) fn print_message(message: &FullMessage) -> Result<PathBuf> {
    let path = unique_file(
        &format!("email-rs-{}", safe_file_stem(&message.id)),
        "html",
        &std::env::temp_dir(),
    )?;
    let html = message_body_text(message)
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    std::fs::write(
        &path,
        format!("<!doctype html><meta charset=\"utf-8\"><pre>{html}</pre>"),
    )?;
    open_path_with_system(&path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use email_types::MessageBody;

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
            attachments: vec![],
        }
    }
}
