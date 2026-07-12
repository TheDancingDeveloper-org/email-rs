use lettre::{
    message::{header::ContentType, Attachment, MultiPart, SinglePart},
    transport::smtp::authentication::{Credentials, Mechanism},
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// For XOAUTH2: the OAuth2 access token. For Basic: the password.
    pub password: String,
    pub use_tls: bool,
    /// Use XOAUTH2 mechanism (Gmail OAuth2) instead of PLAIN/LOGIN.
    pub xoauth2: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub text_body: Option<String>,
    pub html_body: Option<String>,
    pub in_reply_to: Option<String>,
    pub attachments: Vec<OutboundAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundAttachment {
    pub filename: String,
    pub content_type: String,
    pub content: Vec<u8>,
}

fn build_message(msg: &OutboundMessage) -> Result<Message> {
    let from = msg
        .from
        .parse()
        .map_err(|e: lettre::address::AddressError| AppError::Smtp(e.to_string()))?;

    let mut builder = Message::builder().from(from).subject(&msg.subject);

    for to in &msg.to {
        let addr = to
            .parse()
            .map_err(|e: lettre::address::AddressError| AppError::Smtp(e.to_string()))?;
        builder = builder.to(addr);
    }
    for cc in &msg.cc {
        let addr = cc
            .parse()
            .map_err(|e: lettre::address::AddressError| AppError::Smtp(e.to_string()))?;
        builder = builder.cc(addr);
    }
    for bcc in &msg.bcc {
        let addr = bcc
            .parse()
            .map_err(|e: lettre::address::AddressError| AppError::Smtp(e.to_string()))?;
        builder = builder.bcc(addr);
    }

    if let Some(reply_to) = &msg.in_reply_to {
        builder = builder.in_reply_to(reply_to.clone());
    }

    if !msg.attachments.is_empty() {
        let mut mixed = match (&msg.text_body, &msg.html_body) {
            (Some(text), Some(html)) => MultiPart::mixed().multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(text.clone()),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(html.clone()),
                    ),
            ),
            (Some(text), None) => MultiPart::mixed().singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(text.clone()),
            ),
            (None, Some(html)) => MultiPart::mixed().singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_HTML)
                    .body(html.clone()),
            ),
            (None, None) => return Err(AppError::Smtp("message has no body".to_string())),
        };
        for attachment in &msg.attachments {
            let content_type = ContentType::parse(&attachment.content_type)
                .map_err(|e| AppError::Smtp(format!("invalid attachment content type: {e}")))?;
            mixed = mixed.singlepart(
                Attachment::new(attachment.filename.clone())
                    .body(attachment.content.clone(), content_type),
            );
        }
        return builder
            .multipart(mixed)
            .map_err(|e| AppError::Smtp(e.to_string()));
    }

    match (&msg.text_body, &msg.html_body) {
        (Some(text), Some(html)) => builder
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(text.clone()),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(html.clone()),
                    ),
            )
            .map_err(|e| AppError::Smtp(e.to_string())),
        (Some(text), None) => builder
            .body(text.clone())
            .map_err(|e| AppError::Smtp(e.to_string())),
        (None, Some(html)) => builder
            .header(ContentType::TEXT_HTML)
            .body(html.clone())
            .map_err(|e| AppError::Smtp(e.to_string())),
        (None, None) => Err(AppError::Smtp("message has no body".to_string())),
    }
}

pub async fn send_message(config: &SmtpConfig, msg: OutboundMessage) -> Result<()> {
    let email = build_message(&msg)?;

    let creds = Credentials::new(config.username.clone(), config.password.clone());
    let mechanisms = if config.xoauth2 {
        vec![Mechanism::Xoauth2]
    } else {
        vec![Mechanism::Plain, Mechanism::Login]
    };

    let transport = if config.use_tls {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
            .map_err(|e| AppError::Smtp(e.to_string()))?
            .credentials(creds)
            .authentication(mechanisms)
            .build()
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
            .map_err(|e| AppError::Smtp(e.to_string()))?
            .port(config.port)
            .credentials(creds)
            .authentication(mechanisms)
            .build()
    };

    transport
        .send(email)
        .await
        .map_err(|e| AppError::Smtp(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_multipart_message_with_cc_bcc_and_attachment() {
        let message = build_message(&OutboundMessage {
            from: "me@example.test".into(),
            to: vec!["to@example.test".into()],
            cc: vec!["cc@example.test".into()],
            bcc: vec!["bcc@example.test".into()],
            subject: "Attachment test".into(),
            text_body: Some("hello".into()),
            html_body: None,
            in_reply_to: Some("<previous@example.test>".into()),
            attachments: vec![OutboundAttachment {
                filename: "report.txt".into(),
                content_type: "text/plain".into(),
                content: b"attachment body".to_vec(),
            }],
        })
        .unwrap();

        let wire = String::from_utf8(message.formatted()).unwrap();
        assert!(wire.contains("Cc: cc@example.test"));
        assert!(message
            .envelope()
            .to()
            .iter()
            .any(|address| address.to_string() == "bcc@example.test"));
        assert!(!wire.contains("Bcc:"));
        assert!(wire.contains("Content-Disposition: attachment"));
        assert!(wire.contains("filename=\"report.txt\""));
    }
}
