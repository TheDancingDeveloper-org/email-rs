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
    source.push_str(value);
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
    std::fs::write(path, message_source(message))
        .with_context(|| format!("write {}", path.display()))
}

pub(crate) fn open_source(message: &FullMessage) -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("email-rs-{}.eml", message.id));
    write_eml(message, &path)?;
    open_path_with_system(&path)?;
    Ok(path)
}

pub(crate) fn save_eml(message: &FullMessage) -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("{}.eml", message.id));
    write_eml(message, &path)?;
    Ok(path)
}

pub(crate) fn print_message(message: &FullMessage) -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("email-rs-{}.html", message.id));
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
        let path = std::env::temp_dir().join("email-rs-round-trip.eml");
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
}
