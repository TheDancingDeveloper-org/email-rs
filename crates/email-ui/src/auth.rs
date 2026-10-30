//! Sign-in form state and the checks made before a request is sent.

/// Which `app-auth` endpoint the form calls.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AuthMode {
    /// `POST /app-auth/login` for an existing user.
    #[default]
    SignIn,
    /// `POST /app-auth/bootstrap`: creates the first user. The server only
    /// accepts it from loopback and only while no user exists.
    FirstRun,
}

/// The server rejects a first-run password shorter than this
/// (`validate_password` in `email-server`). Checked here too, so the user
/// sees the rule before a round trip.
pub const MIN_PASSWORD_LEN: usize = 12;

/// Username and password typed into the sign-in screen. `Debug` redacts the
/// password.
#[derive(Clone, Default)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    pub mode: AuthMode,
}

/// Credentials ready to send. `Debug` redacts the password.
#[derive(Clone, Eq, PartialEq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    pub mode: AuthMode,
}

/// What `Debug` prints in place of a secret.
pub(crate) const REDACTED: &str = "<redacted>";

impl std::fmt::Debug for LoginForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginForm")
            .field("username", &self.username)
            .field("password", &REDACTED)
            .field("mode", &self.mode)
            .finish()
    }
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("password", &REDACTED)
            .field("mode", &self.mode)
            .finish()
    }
}

impl LoginForm {
    /// Checks the form and returns the credentials to send, or the message
    /// to show. The username is trimmed; the password is sent as typed.
    pub fn submit(&self) -> Result<Credentials, String> {
        let username = self.username.trim();
        if username.is_empty() || self.password.is_empty() {
            return Err("Username and password are required.".into());
        }
        if self.mode == AuthMode::FirstRun && self.password.len() < MIN_PASSWORD_LEN {
            return Err(format!(
                "The password must have at least {MIN_PASSWORD_LEN} characters."
            ));
        }
        Ok(Credentials {
            username: username.to_string(),
            password: self.password.clone(),
            mode: self.mode,
        })
    }

    /// Switches between sign-in and first-run setup.
    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            AuthMode::SignIn => AuthMode::FirstRun,
            AuthMode::FirstRun => AuthMode::SignIn,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(username: &str, password: &str, mode: AuthMode) -> LoginForm {
        LoginForm {
            username: username.into(),
            password: password.into(),
            mode,
        }
    }

    #[test]
    fn empty_fields_are_rejected() {
        assert!(form("", "pw", AuthMode::SignIn).submit().is_err());
        assert!(form("  ", "pw", AuthMode::SignIn).submit().is_err());
        assert!(form("admin", "", AuthMode::SignIn).submit().is_err());
    }

    #[test]
    fn username_is_trimmed_and_password_kept() {
        let creds = form(" admin ", " pw ", AuthMode::SignIn).submit().unwrap();
        assert_eq!(creds.username, "admin");
        assert_eq!(creds.password, " pw ");
        assert_eq!(creds.mode, AuthMode::SignIn);
    }

    #[test]
    fn first_run_needs_a_long_password() {
        assert!(form("admin", "short", AuthMode::FirstRun).submit().is_err());
        assert!(form("admin", "short", AuthMode::SignIn).submit().is_ok());
        let creds = form("admin", "twelve chars", AuthMode::FirstRun)
            .submit()
            .unwrap();
        assert_eq!(creds.mode, AuthMode::FirstRun);
    }

    #[test]
    fn toggle_mode_flips() {
        let mut f = LoginForm::default();
        f.toggle_mode();
        assert_eq!(f.mode, AuthMode::FirstRun);
        f.toggle_mode();
        assert_eq!(f.mode, AuthMode::SignIn);
    }

    #[test]
    fn debug_redacts_the_password() {
        let f = form("admin", "hunter2-secret", AuthMode::SignIn);
        let shown = format!("{f:?}");
        assert!(!shown.contains("hunter2-secret"), "{shown}");
        assert!(shown.contains(REDACTED) && shown.contains("admin"));
        let creds = f.submit().unwrap();
        let shown = format!("{creds:?}");
        assert!(!shown.contains("hunter2-secret"), "{shown}");
        assert!(shown.contains(REDACTED));
    }
}
