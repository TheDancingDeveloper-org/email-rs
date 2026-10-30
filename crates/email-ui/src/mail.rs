//! The read calls the shell makes once signed in: mailboxes, a folder's
//! messages and one full message. Behind a trait so tests run without a
//! server.

use crate::backend::{classify, AuthError, HttpAuth};
use email_client::ClientCredential;
use email_types::{Account, Folder, FullMessage, Message};

/// One account and its folders, in the order the server returns them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mailbox {
    pub account: Account,
    pub folders: Vec<Folder>,
}

/// Blocking read calls, each with the session's bearer token. Run off the
/// UI thread by [`crate::app::EmailApp`].
pub trait MailApi: Send + Sync + 'static {
    /// `GET /accounts`, then `GET /accounts/{id}/folders` for each.
    fn mailboxes(&self, token: &str) -> Result<Vec<Mailbox>, AuthError>;
    /// `GET /folders/{id}/messages`.
    fn messages(&self, token: &str, folder_id: &str) -> Result<Vec<Message>, AuthError>;
    /// `GET /messages/{id}`.
    fn message(&self, token: &str, message_id: &str) -> Result<FullMessage, AuthError>;
}

impl MailApi for HttpAuth {
    fn mailboxes(&self, token: &str) -> Result<Vec<Mailbox>, AuthError> {
        let client = self.client(ClientCredential::bearer(token))?;
        self.runtime().block_on(async {
            let accounts = client.list_accounts().await.map_err(classify)?;
            let mut out = Vec::with_capacity(accounts.len());
            for account in accounts {
                let folders = client.list_folders(&account.id).await.map_err(classify)?;
                out.push(Mailbox { account, folders });
            }
            Ok(out)
        })
    }

    fn messages(&self, token: &str, folder_id: &str) -> Result<Vec<Message>, AuthError> {
        let client = self.client(ClientCredential::bearer(token))?;
        self.runtime()
            .block_on(client.list_folder_messages(folder_id))
            .map_err(classify)
    }

    fn message(&self, token: &str, message_id: &str) -> Result<FullMessage, AuthError> {
        let client = self.client(ClientCredential::bearer(token))?;
        self.runtime()
            .block_on(client.get_message(message_id))
            .map_err(classify)
    }
}

/// The sender as shown in the message list: the name, else the address.
pub fn sender(name: Option<&str>, email: Option<&str>) -> String {
    [name, email]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or("(unknown sender)")
        .to_string()
}

/// The subject as shown, with a placeholder for none.
pub fn subject(subject: Option<&str>) -> String {
    match subject.map(str::trim) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => "(no subject)".to_string(),
    }
}

/// The body to hand to HtmlView: the HTML part, else the text part
/// escaped and kept as preformatted text.
pub fn body_html(message: &FullMessage) -> String {
    let body = message.body.as_ref();
    if let Some(html) = body.and_then(|b| b.html_body.as_deref()) {
        if !html.trim().is_empty() {
            return html.to_string();
        }
    }
    match body.and_then(|b| b.text_body.as_deref()) {
        Some(text) if !text.trim().is_empty() => format!("<pre>{}</pre>", escape(text)),
        _ => String::new(),
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use email_types::MessageBody;

    fn full(html: Option<&str>, text: Option<&str>) -> FullMessage {
        FullMessage {
            id: "m".into(),
            occurrence_id: "m".into(),
            logical_id: "m".into(),
            account_id: "a".into(),
            folder_id: "f".into(),
            uid: 1,
            message_id: None,
            thread_id: None,
            subject: None,
            from_name: None,
            from_email: None,
            to: None,
            date: None,
            is_read: false,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: None,
            body: Some(MessageBody {
                html_body: html.map(Into::into),
                text_body: text.map(Into::into),
                ..Default::default()
            }),
            attachments: vec![],
        }
    }

    #[test]
    fn html_part_wins_and_text_is_escaped() {
        assert_eq!(body_html(&full(Some("<p>hi</p>"), Some("x"))), "<p>hi</p>");
        assert_eq!(
            body_html(&full(Some("  "), Some("a <b> & c"))),
            "<pre>a &lt;b&gt; &amp; c</pre>"
        );
        assert_eq!(body_html(&full(None, None)), "");
    }

    #[test]
    fn sender_and_subject_fall_back() {
        assert_eq!(sender(Some(" "), Some("a@x.test")), "a@x.test");
        assert_eq!(sender(Some("Ann"), Some("a@x.test")), "Ann");
        assert_eq!(sender(None, None), "(unknown sender)");
        assert_eq!(subject(Some("")), "(no subject)");
        assert_eq!(subject(Some("Hi")), "Hi");
    }
}
