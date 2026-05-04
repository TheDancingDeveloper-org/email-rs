use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub frontend_dist: String,
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
                // XDG_DATA_HOME/email-rs/email.db  →  ~/.local/share/email-rs/email.db
                let data_home = env::var("XDG_DATA_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| {
                        env::var("HOME")
                            .map(|h| PathBuf::from(h).join(".local/share"))
                            .unwrap_or_else(|_| PathBuf::from("/tmp"))
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
        }
    }

    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

fn exe_dir() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}
