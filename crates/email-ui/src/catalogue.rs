//! The shared UI message catalogue.
//!
//! Keeping message ids separate from their English text makes the UI ready for
//! adding locales without making every shell invent its own string layer.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MessageId {
    CreateAccount,
    FirstRunDescription,
    FirstRunPrompt,
    FirstRunTitle,
    HaveAccount,
    PasswordLabel,
    PasswordMinLength,
    SignIn,
    SignInFailed,
    SignInTitle,
    SigningIn,
    UsernameLabel,
    CredentialsRequired,
}

impl MessageId {
    pub const ALL: [Self; 13] = [
        Self::CreateAccount,
        Self::FirstRunDescription,
        Self::FirstRunPrompt,
        Self::FirstRunTitle,
        Self::HaveAccount,
        Self::PasswordLabel,
        Self::PasswordMinLength,
        Self::SignIn,
        Self::SignInFailed,
        Self::SignInTitle,
        Self::SigningIn,
        Self::UsernameLabel,
        Self::CredentialsRequired,
    ];
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MessageArgs(BTreeMap<&'static str, String>);

impl MessageArgs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(mut self, name: &'static str, value: impl ToString) -> Self {
        self.0.insert(name, value.to_string());
        self
    }

    /// Sets the `count` argument, which also selects the plural variant.
    pub fn count(self, count: usize) -> Self {
        self.insert("count", count)
    }
}

#[derive(Clone, Debug)]
pub struct Catalogue {
    locale: String,
}

impl Default for Catalogue {
    fn default() -> Self {
        Self::new("en-US")
    }
}

impl Catalogue {
    pub fn new(locale: impl Into<String>) -> Self {
        Self {
            locale: locale.into(),
        }
    }

    /// Looks up a message, falling back to the en-US catalogue. The `count`
    /// argument selects the singular/plural variant where a message has one.
    pub fn get(&self, id: MessageId, args: MessageArgs) -> String {
        let entry = self
            .localized(id)
            .or_else(|| english(id))
            .expect("every MessageId must have an en-US entry");
        let count = args.0.get("count").and_then(|c| c.parse::<usize>().ok());
        let template = match (count, entry.one) {
            (Some(1), Some(one)) => one,
            _ => entry.other,
        };
        args.0
            .iter()
            .fold(template.to_string(), |text, (name, value)| {
                text.replace(&format!("{{{name}}}"), value)
            })
    }

    fn localized(&self, id: MessageId) -> Option<Entry> {
        // This is intentionally the extension point for additional locales.
        // Until they are added, en-US is the authoritative fallback.
        (self.locale == "en-US").then(|| english(id)).flatten()
    }
}

/// One catalogue entry: `other` is the default and plural form; `one` is the
/// en-US singular used when the `count` argument is 1.
#[derive(Clone, Copy)]
struct Entry {
    other: &'static str,
    one: Option<&'static str>,
}

fn plain(other: &'static str) -> Entry {
    Entry { other, one: None }
}

fn english(id: MessageId) -> Option<Entry> {
    Some(match id {
        MessageId::CreateAccount => plain("Create account"),
        MessageId::FirstRunDescription => {
            plain("Create the first account. This works only on the server's own machine.")
        }
        MessageId::FirstRunPrompt => plain("First run? Set up the first account"),
        MessageId::FirstRunTitle => plain("Set up email-rs"),
        MessageId::HaveAccount => plain("I already have an account"),
        MessageId::PasswordLabel => plain("Password"),
        MessageId::PasswordMinLength => Entry {
            other: "The password must have at least {count} characters.",
            one: Some("The password must have at least {count} character."),
        },
        MessageId::SignIn => plain("Sign in"),
        MessageId::SignInFailed => plain("Sign-in failed."),
        MessageId::SignInTitle => plain("Sign in to email-rs"),
        MessageId::SigningIn => plain("Signing in…"),
        MessageId::UsernameLabel => plain("Username"),
        MessageId::CredentialsRequired => plain("Username and password are required."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_an_english_message() {
        let catalogue = Catalogue::default();
        for id in MessageId::ALL {
            assert!(!catalogue.get(id, MessageArgs::new()).is_empty());
        }
    }

    #[test]
    fn plural_variant_follows_count() {
        let catalogue = Catalogue::default();
        let get = |n| catalogue.get(MessageId::PasswordMinLength, MessageArgs::new().count(n));
        assert_eq!(get(1), "The password must have at least 1 character.");
        assert_eq!(get(0), "The password must have at least 0 characters.");
    }

    #[test]
    fn every_key_used_in_code_exists() {
        let known: Vec<String> = MessageId::ALL.iter().map(|i| format!("{i:?}")).collect();
        for source in [include_str!("app.rs"), include_str!("auth.rs")] {
            for part in source.split("MessageId::").skip(1) {
                let name: String = part
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if name == "ALL" || name.is_empty() {
                    continue;
                }
                assert!(known.contains(&name), "MessageId::{name} missing from ALL");
            }
        }
    }

    #[test]
    fn arguments_are_interpolated_and_unknown_locales_fall_back() {
        let catalogue = Catalogue::new("fr-FR");
        assert_eq!(
            catalogue.get(MessageId::PasswordMinLength, MessageArgs::new().count(12)),
            "The password must have at least 12 characters."
        );
    }
}
