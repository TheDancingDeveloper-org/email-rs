#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod api;
mod auth;
mod calendar;
mod config;
mod db;
mod error;
mod imap;
mod providers;
mod rules;
mod smtp;

mod state;
mod sync;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{routing::get_service, Router};
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing::info;
#[cfg(not(target_os = "windows"))]
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::Config;
use crate::state::AppState;
use crate::sync::SyncOrchestrator;

fn main() -> anyhow::Result<()> {
    // Snap injects its own (old) libpthread via LD_LIBRARY_PATH, which conflicts
    // with the system glibc. Strip snap paths and re-exec before any libs load.
    #[cfg(target_os = "linux")]
    strip_snap_ld_path();

    let _ = dotenvy::dotenv();
    let cfg = Config::from_env();

    #[cfg(target_os = "windows")]
    {
        let log_path = exe_log_path();

        // Enforce single instance: if another copy is already running, bring its
        // window to the foreground and exit cleanly rather than crashing with a
        // port-conflict or WebView2 data-dir error.
        let _instance_guard = match win_instance::acquire(&log_path) {
            Some(g) => g,
            None => return Ok(()),
        };

        let panic_log = log_path.clone();
        std::panic::set_hook(Box::new(move |info| {
            win_log(&panic_log, &format!("panic: {info}"));
        }));

        let port = cfg.port;
        let server_log = log_path.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            if let Err(e) = rt.block_on(run_server(cfg)) {
                win_log(&server_log, &format!("server error: {e:#}"));
            }
        });

        if let Err(e) = run_desktop(port, &log_path) {
            win_log(&log_path, &format!("desktop error: {e:#}"));
        }
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
            .with(tracing_subscriber::fmt::layer())
            .init();

        #[cfg(feature = "desktop")]
        {
            let has_display = std::env::var("DISPLAY").is_ok()
                || std::env::var("WAYLAND_DISPLAY").is_ok();
            let headless = std::env::var("EMAIL_RS_HEADLESS").is_ok();

            if has_display && !headless {
                let port = cfg.port;
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
                    if let Err(e) = rt.block_on(run_server(cfg)) {
                        tracing::error!("server error: {e:#}");
                    }
                });
                return run_desktop_linux(port);
            }
        }

        tokio::runtime::Runtime::new()?.block_on(run_server(cfg))?;
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
            .with(tracing_subscriber::fmt::layer())
            .init();
        tokio::runtime::Runtime::new()?.block_on(run_server(cfg))?;
    }

    Ok(())
}

async fn run_server(cfg: Config) -> anyhow::Result<()> {
    info!("starting email-server on {}", cfg.bind_addr());

    let (pool, has_fts) = db::create_pool(&cfg.database_url).await?;
    let app_state = Arc::new(AppState::new(pool.clone(), has_fts));
    SyncOrchestrator::spawn_background(pool, app_state.event_tx.clone());

    let api_routes = api::router(app_state);
    let static_files = ServeDir::new(&cfg.frontend_dist).append_index_html_on_directories(true);

    let app = Router::new()
        .nest("/api/v1", api_routes)
        .fallback_service(get_service(static_files))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = cfg.bind_addr().parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

/// On first startup, re-exec with a clean environment to prevent snap's ancient
/// glibc/libpthread (core20/core22) from being loaded via LD_LIBRARY_PATH or RPATH.
/// Preloading the system libpthread overrides any snap version, regardless of source.
#[cfg(target_os = "linux")]
fn strip_snap_ld_path() {
    use std::os::unix::process::CommandExt;

    // Guard against infinite re-exec loops.
    if std::env::var("_EMAIL_RS_REEXEC").is_ok() {
        return;
    }

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };

    // Strip snap paths from LD_LIBRARY_PATH.
    let ld_path = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
    let cleaned: String = ld_path
        .split(':')
        .filter(|p| !p.is_empty() && !p.starts_with("/snap/"))
        .collect::<Vec<_>>()
        .join(":");

    // Preload the system libpthread stub so it wins over any snap version loaded
    // via RPATH in a GTK/WebKit library.
    let system_pthread = [
        "/lib/x86_64-linux-gnu/libpthread.so.0",
        "/usr/lib/x86_64-linux-gnu/libpthread.so.0",
    ]
    .iter()
    .find(|p| std::path::Path::new(p).exists())
    .copied()
    .unwrap_or("");

    let args: Vec<String> = std::env::args().skip(1).collect();
    let err = std::process::Command::new(&exe)
        .args(&args)
        .env("LD_LIBRARY_PATH", &cleaned)
        .env("LD_PRELOAD", system_pthread)
        .env("_EMAIL_RS_REEXEC", "1")
        // Disable DMA-BUF renderer — causes blank WebKitGTK windows on many setups.
        .env("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
        // Disable GPU compositing — WebKit 2.48+ on Wayland renders nothing without this.
        .env("WEBKIT_DISABLE_COMPOSITING_MODE", "1")
        // WebKit 2.52+ sandbox blocks the render process from accessing the display,
        // causing a blank window.
        .env("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS", "1")
        .exec();
    // exec() only returns on error.
    eprintln!("warning: re-exec failed: {err}");
}

#[cfg(all(target_os = "linux", feature = "desktop"))]
fn run_desktop_linux(port: u16) -> anyhow::Result<()> {
    use gdk_pixbuf::PixbufLoader;
    use gtk::glib;
    use gtk::prelude::*;
    use webkit2gtk::{WebContext, WebView, WebViewExt, WebsiteDataManager};

    gtk::init()?;

    wait_for_server_ready_linux(port);

    let url = format!("http://localhost:{port}");

    // Build a WebContext with a per-user data directory.
    let data_dir = {
        let base = std::env::var("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("HOME")
                    .map(|h| std::path::PathBuf::from(h).join(".local/share"))
                    .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            });
        base.join("email-rs/webview")
    };
    let _ = std::fs::create_dir_all(&data_dir);

    let data_manager = WebsiteDataManager::builder()
        .base_data_directory(data_dir.to_str().unwrap_or("/tmp"))
        .build();
    let ctx = WebContext::with_website_data_manager(&data_manager);
    let webview = WebView::with_context(&ctx);
    webview.load_uri(&url);

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("email-rs");
    window.set_default_size(1280, 800);

    // Set window icon from embedded PNG.
    let icon_bytes = include_bytes!("../icons/mail_32.png");
    let loader = PixbufLoader::with_type("png").expect("png loader");
    loader.write(icon_bytes).ok();
    loader.close().ok();
    if let Some(pixbuf) = loader.pixbuf() {
        window.set_icon(Some(&pixbuf));
    }

    window.add(&webview);
    window.show_all();

    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });

    gtk::main();
    Ok(())
}

#[cfg(all(target_os = "linux", feature = "desktop"))]
fn wait_for_server_ready_linux(port: u16) {
    use std::time::Duration;
    let addr = format!("127.0.0.1:{port}");
    for _ in 0..100 {
        if std::net::TcpStream::connect(&addr).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    tracing::warn!("server did not become ready within 10s — opening window anyway");
}

#[cfg(target_os = "windows")]
fn run_desktop(port: u16, log: &std::path::Path) -> anyhow::Result<()> {
    use std::time::{Duration, Instant};
    use tao::{
        dpi::LogicalSize,
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
        window::WindowBuilder,
    };
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
        MouseButton, TrayIconBuilder, TrayIconEvent,
    };
    use wry::{WebContext, WebViewBuilder};

    let url = format!("http://localhost:{port}");

    let open_item = MenuItem::new("Open email-rs", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    let open_id = open_item.id().clone();
    let quit_id = quit_item.id().clone();

    let menu = Menu::new();
    menu.append_items(&[&open_item, &PredefinedMenuItem::separator(), &quit_item])?;

    let icon_bytes = include_bytes!("../icons/mail_32.png");
    let icon_img = image::load_from_memory(icon_bytes)
        .expect("valid mail_32.png")
        .to_rgba8();
    let (icon_w, icon_h) = icon_img.dimensions();
    let icon_rgba = icon_img.into_raw();

    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("email-rs")
        .with_icon(tray_icon::Icon::from_rgba(icon_rgba, icon_w, icon_h)?)
        .build()?;

    // Wait for the Axum server to bind before opening the window
    wait_for_server_ready(port, log);

    let window_icon = {
        let img = image::load_from_memory(include_bytes!("../icons/mail_32.png"))
            .expect("valid mail_32.png")
            .to_rgba8();
        let (w, h) = img.dimensions();
        tao::window::Icon::from_rgba(img.into_raw(), w, h).ok()
    };

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("email-rs")
        .with_window_icon(window_icon)
        .with_inner_size(LogicalSize::new(1280u32, 800u32))
        .with_min_inner_size(LogicalSize::new(800u32, 600u32))
        .build(&event_loop)?;

    // WebView2 defaults to creating its user-data folder next to the exe.
    // When installed under `C:\Program Files\email-rs` that path is read-only
    // and WebView2 fails with HRESULT 0x80070005 (Access is denied). Point it
    // at a writable per-user location instead.
    let data_dir = webview_data_dir();
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        win_log(
            log,
            &format!("could not create webview data dir {data_dir:?}: {e}"),
        );
    }

    let mut web_context = WebContext::new(Some(data_dir));

    let _webview = WebViewBuilder::with_web_context(&mut web_context)
        .with_url(&url)
        .build(&window)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50));

        if let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == open_id {
                window.set_visible(true);
            } else if ev.id == quit_id {
                *control_flow = ControlFlow::Exit;
            }
        }

        if let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = ev
            {
                window.set_visible(true);
            }
        }

        // Close button minimises to tray rather than quitting
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            window.set_visible(false);
        }
    });
}

#[cfg(target_os = "windows")]
fn wait_for_server_ready(port: u16, log: &std::path::Path) {
    use std::time::Duration;
    let addr = format!("127.0.0.1:{port}");
    for _ in 0..100 {
        if std::net::TcpStream::connect(&addr).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    win_log(
        log,
        "server did not become ready within 10s — opening window anyway",
    );
}

#[cfg(target_os = "windows")]
fn webview_data_dir() -> std::path::PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("APPDATA"))
        .unwrap_or_else(|_| std::env::var("TEMP").unwrap_or_else(|_| "C:\\Windows\\Temp".into()));
    std::path::PathBuf::from(base)
        .join("email-rs")
        .join("webview")
}

#[cfg(target_os = "windows")]
fn exe_log_path() -> std::path::PathBuf {
    // Program Files is read-only for normal users; TEMP is always writable
    let base = std::env::var("TEMP")
        .or_else(|_| std::env::var("TMP"))
        .unwrap_or_else(|_| "C:\\Windows\\Temp".into());
    std::path::PathBuf::from(base).join("email-rs.log")
}

#[cfg(target_os = "windows")]
fn win_log(path: &std::path::Path, msg: &str) {
    use std::io::Write;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("[{ts}] {msg}\n");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Single-instance enforcement via a named kernel mutex.
///
/// `acquire` returns `Some(guard)` for the first instance. All subsequent
/// instances get `None` — the existing window is brought to the foreground
/// before returning so the user sees the app immediately.
///
/// The guard must be kept alive for the entire process lifetime (binding it to
/// `_instance_guard` in `main` is enough).
#[cfg(target_os = "windows")]
mod win_instance {
    use std::ffi::c_void;

    // Raw Win32 imports — kernel32 + user32 are always available on Windows.
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(attrs: *const c_void, owner: u32, name: *const u16) -> *mut c_void;
        fn GetLastError() -> u32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn FindWindowW(class_name: *const u16, window_name: *const u16) -> *mut c_void;
        fn IsIconic(hwnd: *mut c_void) -> i32;
        fn ShowWindow(hwnd: *mut c_void, n_cmd_show: i32) -> i32;
        fn SetForegroundWindow(hwnd: *mut c_void) -> i32;
    }

    const ERROR_ALREADY_EXISTS: u32 = 183;
    const SW_RESTORE: i32 = 9;
    const SW_SHOW: i32 = 5;

    pub struct Guard(*mut c_void);

    // Safety: the handle is only closed on drop; no other thread touches it.
    unsafe impl Send for Guard {}

    impl Drop for Guard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    fn utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Try to become the single running instance.
    /// Returns `Some(guard)` on success, `None` if another instance is running.
    pub fn acquire(log: &std::path::Path) -> Option<Guard> {
        let name = utf16("Local\\email-rs-singleton-v1");
        let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };

        if handle.is_null() || unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            if !handle.is_null() {
                unsafe { CloseHandle(handle) };
            }
            // Bring the existing window to the foreground.
            let title = utf16("email-rs");
            let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
            if !hwnd.is_null() {
                unsafe {
                    if IsIconic(hwnd) != 0 {
                        ShowWindow(hwnd, SW_RESTORE);
                    } else {
                        ShowWindow(hwnd, SW_SHOW);
                    }
                    SetForegroundWindow(hwnd);
                }
            }
            super::win_log(log, "another instance is already running — exiting");
            return None;
        }

        Some(Guard(handle))
    }
}
