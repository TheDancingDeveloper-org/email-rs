//! `email-ui`: the fluent2-egui desktop client.
//!
//! Connects to a running email-rs server, `EMAIL_RS_SERVER_URL` or
//! `http://localhost:8585` (the server's default port).

use email_ui::{EmailApp, Exec, HttpAuth, KeyringStore};
use std::sync::Arc;

const DEFAULT_SERVER_URL: &str = "http://localhost:8585";

fn main() {
    let base_url = std::env::var("EMAIL_RS_SERVER_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string());
    let api = match HttpAuth::new(base_url.clone()) {
        Ok(api) => api,
        Err(err) => {
            eprintln!("email-ui: could not start the I/O runtime: {err}");
            std::process::exit(1);
        }
    };
    let store = KeyringStore::for_server(&base_url);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("email-rs")
            .with_inner_size([1100.0, 720.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "email-rs",
        options,
        Box::new(move |cc| {
            EmailApp::install_theme(&cc.egui_ctx);
            Ok(Box::new(EmailApp::new(
                Arc::new(api),
                Box::new(store),
                Exec::Thread,
            )))
        }),
    );
    if let Err(err) = result {
        eprintln!("email-ui: {err}");
        std::process::exit(1);
    }
}
