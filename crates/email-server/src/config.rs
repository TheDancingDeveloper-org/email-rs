use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub frontend_dist: String,
    pub public_url: Option<String>,
    pub allow_insecure_http: bool,
    /// `EMAIL_RS_DISABLE_AUTH`: turns off all authentication.
    pub disable_auth: bool,
    /// `EMAIL_RS_ALLOW_OPEN_NO_AUTH`: second opt-in to run without auth on a non-loopback bind.
    pub allow_open_no_auth: bool,
    pub webhook_policy: WebhookPolicy,
    pub allow_corrupt_database: bool,
    pub maintenance: MaintenanceConfig,
}

#[derive(Debug, Clone)]
pub struct MaintenanceConfig {
    pub orphan_cleanup_interval: std::time::Duration,
    pub vacuum_interval: std::time::Duration,
    pub fts_optimize_interval: std::time::Duration,
}

impl MaintenanceConfig {
    fn from_env() -> Self {
        fn seconds(name: &str, default: u64) -> std::time::Duration {
            std::time::Duration::from_secs(
                env::var(name)
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(default)
                    .max(1),
            )
        }

        Self {
            orphan_cleanup_interval: seconds("EMAIL_RS_ORPHAN_CLEANUP_INTERVAL_SECS", 86_400),
            vacuum_interval: seconds("EMAIL_RS_INCREMENTAL_VACUUM_INTERVAL_SECS", 86_400),
            fts_optimize_interval: seconds("EMAIL_RS_FTS_OPTIMIZE_INTERVAL_SECS", 604_800),
        }
    }
}

/// Controls which webhook targets email-rs will store and call.
///
/// Private, loopback, link-local and metadata addresses are refused after DNS
/// resolution unless the exact URL is in `allowlist`. Plain `http` is refused
/// unless `allow_insecure_http` is set (the same development flag that permits
/// a non-loopback bind without HTTPS).
#[derive(Debug, Clone)]
pub struct WebhookPolicy {
    pub allow_insecure_http: bool,
    /// Exact webhook URLs that skip the private-address check. Empty by default.
    pub allowlist: Vec<String>,
}

impl WebhookPolicy {
    pub fn from_env() -> Self {
        let allowlist = env::var("EMAIL_RS_WEBHOOK_ALLOWLIST")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(str::to_owned)
            .collect();
        Self {
            allow_insecure_http: env_flag("EMAIL_RS_ALLOW_INSECURE_HTTP"),
            allowlist,
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let frontend_dist = env::var("FRONTEND_DIST").unwrap_or_else(|_| {
            // 1. Next to the executable (Docker image, packaged install)
            let beside_exe = exe_dir().join("static");
            if beside_exe.exists() {
                return beside_exe.to_string_lossy().into_owned();
            }
            // 2. Standard Linux system install paths
            for system_path in &[
                PathBuf::from("/usr/local/share/email-rs/static"),
                PathBuf::from("/usr/share/email-rs/static"),
            ] {
                if system_path.exists() {
                    return system_path.to_string_lossy().into_owned();
                }
            }
            // 3. Source-tree location for local `cargo build` runs
            let in_src = env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("crates/email-server/static");
            if in_src.exists() {
                return in_src.to_string_lossy().into_owned();
            }
            beside_exe.to_string_lossy().into_owned()
        });

        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            #[cfg(target_os = "windows")]
            {
                let base = env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
                format!("sqlite:{}/email-rs/email.db", base.replace('\\', "/"))
            }
            #[cfg(not(target_os = "windows"))]
            {
                // No /tmp fallback: plaintext mail must not land in a
                // directory shared with every local user.
                let data_home = env::var("XDG_DATA_HOME")
                    .map(PathBuf::from)
                    .or_else(|_| {
                        env::var("HOME").map(|home| PathBuf::from(home).join(".local/share"))
                    })
                    .unwrap_or_else(|_| {
                        panic!(
                            "no data directory configured: set DATABASE_URL, or XDG_DATA_HOME or HOME (email-rs will not use /tmp)"
                        )
                    });
                format!(
                    "sqlite://{}",
                    data_home.join("email-rs/email.db").to_string_lossy()
                )
            }
        });

        Self {
            host: env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8585),
            database_url,
            frontend_dist,
            public_url: env::var("EMAIL_RS_PUBLIC_URL").ok(),
            allow_insecure_http: env_flag("EMAIL_RS_ALLOW_INSECURE_HTTP"),
            disable_auth: env_flag("EMAIL_RS_DISABLE_AUTH"),
            allow_open_no_auth: env_flag("EMAIL_RS_ALLOW_OPEN_NO_AUTH"),
            webhook_policy: WebhookPolicy::from_env(),
            allow_corrupt_database: env_flag("EMAIL_RS_ALLOW_CORRUPT_DATABASE"),
            maintenance: MaintenanceConfig::from_env(),
        }
    }

    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn validate_security(&self) -> anyhow::Result<()> {
        let loopback = matches!(self.host.as_str(), "127.0.0.1" | "::1" | "localhost");
        let public_https = self
            .public_url
            .as_deref()
            .is_some_and(|url| url.starts_with("https://"));
        if !loopback && !public_https && !self.allow_insecure_http {
            anyhow::bail!(
                "non-loopback deployments require EMAIL_RS_PUBLIC_URL=https://...; set EMAIL_RS_ALLOW_INSECURE_HTTP=true only for explicit development"
            );
        }
        if self.disable_auth {
            if !loopback && !self.allow_open_no_auth {
                anyhow::bail!(
                    "EMAIL_RS_DISABLE_AUTH turns off all authentication and the bind address {} is not loopback; bind to 127.0.0.1 or also set EMAIL_RS_ALLOW_OPEN_NO_AUTH=true to accept an open server",
                    self.host
                );
            }
            tracing::warn!(
                "EMAIL_RS_DISABLE_AUTH is set: ALL AUTHENTICATION IS DISABLED; anyone who can reach {} has full access to all mail",
                self.bind_addr()
            );
        }
        Ok(())
    }
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

fn exe_dir() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod data_dir_tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        saved: Vec<(&'static str, Option<String>)>,
    }

    impl EnvGuard {
        fn set(pairs: &[(&str, Option<&str>)]) -> Self {
            let saved = ["DATABASE_URL", "XDG_DATA_HOME", "HOME"]
                .into_iter()
                .map(|name| (name, std::env::var(name).ok()))
                .collect();
            for (name, value) in pairs {
                match value {
                    Some(value) => unsafe { std::env::set_var(name, value) },
                    None => unsafe { std::env::remove_var(name) },
                }
            }
            Self { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (name, value) in &self.saved {
                match value {
                    Some(value) => unsafe { std::env::set_var(name, value) },
                    None => unsafe { std::env::remove_var(name) },
                }
            }
        }
    }

    #[test]
    fn database_url_from_env_is_used_verbatim() {
        let _lock = ENV_LOCK.lock().unwrap();
        let url = "sqlite:///var/lib/email-rs/email.db";
        let _guard = EnvGuard::set(&[("DATABASE_URL", Some(url))]);
        assert_eq!(Config::from_env().database_url, url);
    }

    #[test]
    fn missing_data_dir_env_fails_instead_of_tmp() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvGuard::set(&[
            ("DATABASE_URL", None),
            ("XDG_DATA_HOME", None),
            ("HOME", None),
        ]);
        let result = std::panic::catch_unwind(Config::from_env);
        assert!(
            result.is_err(),
            "from_env returned a config with no data dir"
        );
    }
}

#[cfg(test)]
mod validate_security_tests {
    use super::*;

    fn cfg(host: &str, disable_auth: bool, allow_open: bool) -> Config {
        Config {
            host: host.into(),
            port: 8585,
            database_url: String::new(),
            frontend_dist: String::new(),
            public_url: Some("https://mail.example.com".into()),
            allow_insecure_http: false,
            disable_auth,
            allow_open_no_auth: allow_open,
            webhook_policy: WebhookPolicy {
                allow_insecure_http: false,
                allowlist: vec![],
            },
            allow_corrupt_database: false,
            maintenance: MaintenanceConfig::from_env(),
        }
    }

    #[test]
    fn disable_auth_refused_on_public_bind() {
        assert!(cfg("0.0.0.0", true, false).validate_security().is_err());
    }

    #[test]
    fn disable_auth_allowed_on_loopback() {
        assert!(cfg("127.0.0.1", true, false).validate_security().is_ok());
    }

    #[test]
    fn disable_auth_allowed_public_with_second_opt_in() {
        assert!(cfg("0.0.0.0", true, true).validate_security().is_ok());
    }

    #[test]
    fn auth_enabled_public_bind_unaffected() {
        assert!(cfg("0.0.0.0", false, false).validate_security().is_ok());
    }
}
