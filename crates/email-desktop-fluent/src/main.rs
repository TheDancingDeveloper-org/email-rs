use std::collections::{BTreeMap, BTreeSet};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context as _, Result};
use chrono::{Datelike as _, Duration as ChronoDuration, Local};
use email_client::EmailClient;
use email_server::{run_server, Config};
use email_types::{Account, Folder, FullMessage, Label, Message, SyncEvent};
use fluent_app::{FluentApp, WindowTitle};
use fluent_core::{AsyncResource, ResourceRequest, Theme, ThemeProvider as _};
use fluent_layout::{
    CommandPalette, DockPanel, DockPosition, DocumentViewer, ModalHost, PaletteEntry, Pane,
    PaneConstraints, PaneGroup, RichDocument, SelectableList, SelectableListItem, Workspace,
};
use fluent_primitives::{
    DateTimePicker, DateTimeValue, ProgressBar, TokenInput, TokenInputSuggestion,
};
use gpui::{
    actions, div, prelude::*, px, App, Context, Entity, FontWeight, IntoElement, KeyBinding,
    Render, SharedString, Window,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

actions!(
    email_desktop_fluent,
    [
        TogglePalette,
        SelectMailView,
        SelectCalendarView,
        ApplyLightTheme,
        ApplyDarkTheme,
        IncreaseDensity,
        DecreaseDensity
    ]
);

const APP_TITLE: &str = "email-rs FluentGUI";
const APP_ID: &str = "org.email-rs.desktop.fluent";
const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(10);

fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    init_tracing();

    let cfg = Config::from_env();
    if should_run_headless() {
        tracing::info!("starting email-desktop-fluent in headless server-only mode");
        tokio::runtime::Runtime::new()?.block_on(run_server(cfg))?;
        return Ok(());
    }

    let port = cfg.port;
    let bind_addr = cfg.bind_addr();
    let server_errors = start_server_thread(cfg);
    wait_for_server(&bind_addr, &server_errors)?;

    let base_url = format!("http://localhost:{port}");
    let initial_state = load_initial_state(&base_url)?;

    FluentApp::new(native_window_title(&initial_state))
        .app_id(APP_ID)
        .window_size(1280.0, 800.0)
        .window_min_size(800.0, 600.0)
        .window_status(native_window_status(&initial_state))
        .dark_theme()
        .run(move |cx| {
            cx.bind_keys([
                KeyBinding::new("ctrl-k", TogglePalette, None),
                KeyBinding::new("ctrl-shift-l", ApplyLightTheme, None),
                KeyBinding::new("ctrl-shift-d", ApplyDarkTheme, None),
                KeyBinding::new("ctrl-plus", IncreaseDensity, None),
                KeyBinding::new("ctrl-minus", DecreaseDensity, None),
            ]);

            let state_entity = cx.new(|_| initial_state.clone());

            let modal_host = cx.new(ModalHost::new);
            let sidebar = cx.new({
                let state = state_entity.clone();
                move |cx| SidebarPlaceholder::new(cx, state.clone())
            });
            let sidebar_dock = cx.new(|cx| {
                DockPanel::new(cx, DockPosition::Left, "Mail")
                    .size(280.0)
                    .content(sidebar.clone().into())
            });

            let message_list = cx.new({
                let state = state_entity.clone();
                move |cx| MessageListPlaceholder::new(cx, state.clone())
            });
            let reading_pane = cx.new({
                let state = state_entity.clone();
                move |cx| ReadingPanePlaceholder::new(cx, state.clone())
            });
            let mail_split = cx.new(|_| {
                PaneGroup::horizontal()
                    .first(message_list.clone().into())
                    .second(reading_pane.clone().into())
                    .ratio(0.42)
                    .constraints(
                        PaneConstraints::new()
                            .first_min(320.0)
                            .second_min(420.0)
                            .first_max(Some(640.0)),
                    )
            });
            let pane = cx.new(|cx| Pane::new(cx));
            pane.update(cx, |pane, cx| {
                pane.set_content(mail_split.clone().into(), cx);
            });

            let status_bar = cx.new({
                let state = state_entity.clone();
                move |cx| StatusBar::new(cx, state.clone())
            });

            let workspace = cx.new(|cx| {
                Workspace::new(cx)
                    .left_dock(sidebar_dock.clone())
                    .content(pane.clone().into())
                    .modal_host(modal_host.clone())
                    .status_bar(status_bar.clone().into())
                    .with_file_menu("email-rs")
            });

            let palette = cx.new({
                let state = state_entity.read(cx).clone();
                move |cx| {
                    let mut palette = CommandPalette::new(cx).max_results(14);
                    palette.set_entries(palette_entries(&state), cx);
                    palette
                }
            });

            cx.new(|cx| EmailDesktopApp::new(cx, state_entity.clone(), workspace, palette))
        });

    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .try_init();
}

fn should_run_headless() -> bool {
    if std::env::var("EMAIL_RS_HEADLESS").is_ok() {
        return true;
    }

    #[cfg(target_os = "linux")]
    {
        std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err()
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

fn start_server_thread(cfg: Config) -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(err) => {
                tracing::error!("failed to create server runtime: {err:#}");
                let _ = tx.send(format!("failed to create server runtime: {err:#}"));
                return;
            }
        };

        if let Err(err) = rt.block_on(run_server(cfg)) {
            tracing::error!("email-server exited: {err:#}");
            let _ = tx.send(format!("email-server exited: {err:#}"));
        }
    });
    rx
}

fn wait_for_server(bind_addr: &str, server_errors: &Receiver<String>) -> Result<()> {
    let deadline = Instant::now() + SERVER_READY_TIMEOUT;
    while Instant::now() < deadline {
        match server_errors.try_recv() {
            Ok(err) => return Err(anyhow!(err)),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                return Err(anyhow!("email-server exited before readiness"));
            }
        }

        if TcpStream::connect(bind_addr).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    Err(anyhow!(
        "email-server did not become ready at {bind_addr} within {:?}",
        SERVER_READY_TIMEOUT
    ))
}

fn start_sync_events_thread(base_url: String, updates: Sender<NativeSyncUpdate>) {
    thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(err) => {
                let _ = updates.send(NativeSyncUpdate::Error(format!(
                    "failed to create sync event runtime: {err:#}"
                )));
                return;
            }
        };

        loop {
            if updates.send(NativeSyncUpdate::Connecting).is_err() {
                break;
            }

            let message = match rt.block_on(stream_sync_events(&base_url, &updates)) {
                Ok(()) => "sync event stream closed".to_string(),
                Err(err) => err.to_string(),
            };
            if updates.send(NativeSyncUpdate::Error(message)).is_err() {
                break;
            }

            thread::sleep(Duration::from_secs(5));
        }
    });
}

async fn stream_sync_events(base_url: &str, updates: &Sender<NativeSyncUpdate>) -> Result<()> {
    let client = EmailClient::new(base_url)?;
    let mut response = client.open_sync_events().await?;
    updates
        .send(NativeSyncUpdate::Connected)
        .map_err(|_| anyhow!("sync event listener stopped"))?;

    let mut buffer = String::new();
    while let Some(chunk) = response.chunk().await? {
        let chunk = String::from_utf8_lossy(&chunk);
        for event in drain_sse_sync_events(&mut buffer, chunk.as_ref()) {
            match event {
                Ok(event) => updates
                    .send(NativeSyncUpdate::Event(event))
                    .map_err(|_| anyhow!("sync event listener stopped"))?,
                Err(err) => updates
                    .send(NativeSyncUpdate::Error(err))
                    .map_err(|_| anyhow!("sync event listener stopped"))?,
            }
        }
    }

    Err(anyhow!("sync event stream closed"))
}

fn drain_sse_sync_events(buffer: &mut String, chunk: &str) -> Vec<Result<SyncEvent, String>> {
    buffer.push_str(chunk);
    let mut events = Vec::new();

    while let Some((index, delimiter_len)) = sse_event_boundary(buffer) {
        let block = buffer[..index].to_string();
        buffer.drain(..index + delimiter_len);
        if let Some(event) = parse_sse_sync_event_block(&block) {
            events.push(event);
        }
    }

    events
}

fn sse_event_boundary(buffer: &str) -> Option<(usize, usize)> {
    ["\r\n\r\n", "\n\n", "\r\r"]
        .iter()
        .filter_map(|delimiter| buffer.find(delimiter).map(|index| (index, delimiter.len())))
        .min_by_key(|(index, _)| *index)
}

fn parse_sse_sync_event_block(block: &str) -> Option<Result<SyncEvent, String>> {
    let block = block.replace("\r\n", "\n").replace('\r', "\n");
    let data = block
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");

    if data.trim().is_empty() {
        return None;
    }

    Some(email_client::parse_sync_event(&data).map_err(|err| err.to_string()))
}

fn load_initial_state(base_url: &str) -> Result<NativeAppState> {
    let client = EmailClient::new(base_url)?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut state = NativeAppState::default();
        state.api_base_url = base_url.into();

        match load_accounts(&client).await {
            Ok(mut loaded) => {
                loaded.api_base_url = state.api_base_url;
                Ok(loaded)
            }
            Err(err) => {
                let message = err.to_string();
                state.accounts_resource = AsyncResource::failed(message.clone());
                state.messages_resource = AsyncResource::failed(message.clone());
                state.last_error = Some(message);
                Ok(state)
            }
        }
    })
}

async fn load_accounts(client: &EmailClient) -> Result<NativeAppState> {
    let accounts = client.list_accounts().await.context("list accounts")?;
    let mut folders_by_account = BTreeMap::new();
    let mut labels = Vec::new();

    for account in &accounts {
        let folders = client
            .list_folders(&account.id)
            .await
            .with_context(|| format!("list folders for {}", account.id))?;
        folders_by_account.insert(account.id.clone(), folders);

        match client.list_labels(&account.id).await {
            Ok(mut account_labels) => labels.append(&mut account_labels),
            Err(err) => tracing::warn!("failed to load labels for {}: {err}", account.id),
        }
    }

    let selected_folder = folders_by_account
        .values()
        .flat_map(|folders| folders.iter())
        .find(|folder| folder.special_use.as_deref() == Some("inbox"))
        .or_else(|| {
            folders_by_account
                .values()
                .flat_map(|folders| folders.iter())
                .next()
        })
        .map(|folder| folder.id.clone());

    let mut messages = if let Some(folder_id) = &selected_folder {
        client
            .list_folder_messages(folder_id)
            .await
            .with_context(|| format!("list messages for {folder_id}"))?
    } else {
        Vec::new()
    };

    let selected_message = messages.first().map(|message| message.id.clone());
    let selected_full_message = if let Some(message_id) = selected_message.as_deref() {
        match client.get_message(message_id).await {
            Ok(full_message) => {
                let was_unread = messages
                    .iter()
                    .find(|message| message.id == full_message.id)
                    .is_some_and(|message| !message.is_read);
                if let Some(summary) = messages
                    .iter_mut()
                    .find(|message| message.id == full_message.id)
                {
                    summary.is_read = full_message.is_read;
                }
                if was_unread && full_message.is_read {
                    decrement_folder_unread(&mut folders_by_account, &full_message.folder_id);
                }
                Some(full_message)
            }
            Err(err) => {
                tracing::warn!("failed to load initial selected message {message_id}: {err}");
                None
            }
        }
    } else {
        None
    };
    let accounts_resource = AsyncResource::ready(accounts.clone());
    let messages_resource = AsyncResource::ready(messages.clone());
    let selected_message_resource = selected_full_message
        .clone()
        .map(AsyncResource::ready)
        .unwrap_or_else(AsyncResource::new);

    Ok(NativeAppState {
        accounts,
        accounts_resource,
        folders_by_account,
        labels,
        messages,
        messages_resource,
        selected_folder,
        selected_message,
        selected_full_message,
        selected_message_resource,
        ..NativeAppState::default()
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ThemeMode {
    Dark,
    Light,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SelectedView {
    Mail,
    Calendar,
}

#[derive(Clone, Debug)]
struct DensityTokens {
    level: u8,
    sidebar_row_height: f32,
    message_row_height: f32,
}

impl DensityTokens {
    fn new(level: u8) -> Self {
        let level = level.min(8);
        let relaxed = level as f32;
        Self {
            level,
            sidebar_row_height: 28.0 + relaxed * 2.0,
            message_row_height: 52.0 + relaxed * 4.0,
        }
    }
}

impl Default for DensityTokens {
    fn default() -> Self {
        Self::new(4)
    }
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
struct ComposeState {
    open: bool,
    mode: Option<String>,
    account_id: Option<String>,
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
struct ModalStates {
    account_setup_open: bool,
    settings_open: bool,
    advanced_search_open: bool,
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
struct PaletteState {
    open: bool,
    query: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct NativeSyncStatus {
    connected: bool,
    syncing: bool,
    account_id: Option<String>,
    folder: Option<String>,
    done: i64,
    total: i64,
    last_message: Option<String>,
    last_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NativeSyncUpdate {
    Connecting,
    Connected,
    Event(SyncEvent),
    Error(String),
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct NativeAppState {
    api_base_url: String,
    theme: ThemeMode,
    density: DensityTokens,
    selected_view: SelectedView,
    selected_folder: Option<String>,
    selected_label: Option<String>,
    selected_message: Option<String>,
    selected_full_message: Option<FullMessage>,
    selected_message_resource: AsyncResource<FullMessage, String>,
    selected_event: Option<String>,
    messages: Vec<Message>,
    messages_resource: AsyncResource<Vec<Message>, String>,
    accounts: Vec<Account>,
    accounts_resource: AsyncResource<Vec<Account>, String>,
    labels: Vec<Label>,
    folders_by_account: BTreeMap<String, Vec<Folder>>,
    compose: ComposeState,
    modals: ModalStates,
    palette: PaletteState,
    sync: NativeSyncStatus,
    last_error: Option<String>,
}

impl Default for NativeAppState {
    fn default() -> Self {
        Self {
            api_base_url: String::new(),
            theme: ThemeMode::Dark,
            density: DensityTokens::default(),
            selected_view: SelectedView::Mail,
            selected_folder: None,
            selected_label: None,
            selected_message: None,
            selected_full_message: None,
            selected_message_resource: AsyncResource::new(),
            selected_event: None,
            messages: Vec::new(),
            messages_resource: AsyncResource::new(),
            accounts: Vec::new(),
            accounts_resource: AsyncResource::new(),
            labels: Vec::new(),
            folders_by_account: BTreeMap::new(),
            compose: ComposeState::default(),
            modals: ModalStates::default(),
            palette: PaletteState::default(),
            sync: NativeSyncStatus::default(),
            last_error: None,
        }
    }
}

struct EmailDesktopApp {
    state: Entity<NativeAppState>,
    workspace: Entity<Workspace>,
    palette: Entity<CommandPalette>,
    sync_updates: Receiver<NativeSyncUpdate>,
}

impl EmailDesktopApp {
    fn new(
        cx: &mut Context<Self>,
        state: Entity<NativeAppState>,
        workspace: Entity<Workspace>,
        palette: Entity<CommandPalette>,
    ) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let state_snapshot = state.read(cx).clone();
        WindowTitle::set_title(cx, native_window_title(&state_snapshot));
        WindowTitle::set_status(cx, native_window_status(&state_snapshot));
        let (sync_tx, sync_updates) = mpsc::channel();
        start_sync_events_thread(state_snapshot.api_base_url.clone(), sync_tx);
        cx.spawn(
            |app: gpui::WeakEntity<Self>, async_cx: &mut gpui::AsyncApp| {
                let mut async_cx = async_cx.clone();
                async move {
                    loop {
                        async_cx
                            .background_executor()
                            .timer(Duration::from_millis(250))
                            .await;
                        if app
                            .update(&mut async_cx, |app, cx| app.drain_sync_updates(cx))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            },
        )
        .detach();
        Self {
            state,
            workspace,
            palette,
            sync_updates,
        }
    }

    fn drain_sync_updates(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        loop {
            match self.sync_updates.try_recv() {
                Ok(update) => {
                    changed = true;
                    self.state.update(cx, |state, cx| {
                        apply_sync_update(&mut state.sync, update);
                        cx.notify();
                    });
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    changed = true;
                    self.state.update(cx, |state, cx| {
                        state.sync.connected = false;
                        state.sync.last_error =
                            Some("Sync event listener disconnected".to_string());
                        cx.notify();
                    });
                    break;
                }
            }
        }
        if changed {
            cx.notify();
        }
    }
}

impl Render for EmailDesktopApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("email-desktop-fluent-root")
            .size_full()
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &TogglePalette, _, cx| {
                    app.state.update(cx, |state, cx| {
                        state.palette.open = true;
                        cx.notify();
                    });
                    app.palette.update(cx, |palette, cx| palette.show(cx));
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &ApplyLightTheme, _, cx| {
                    app.state.update(cx, |state, cx| {
                        state.theme = ThemeMode::Light;
                        cx.notify();
                    });
                    Theme::apply_light(cx);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &ApplyDarkTheme, _, cx| {
                    app.state.update(cx, |state, cx| {
                        state.theme = ThemeMode::Dark;
                        cx.notify();
                    });
                    Theme::apply_dark(cx);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &SelectMailView, _, cx| {
                    app.state.update(cx, |state, cx| {
                        state.selected_view = SelectedView::Mail;
                        cx.notify();
                    });
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &SelectCalendarView, _, cx| {
                    app.state.update(cx, |state, cx| {
                        state.selected_view = SelectedView::Calendar;
                        cx.notify();
                    });
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &IncreaseDensity, _, cx| {
                    app.state.update(cx, |state, cx| {
                        state.density = DensityTokens::new(state.density.level.saturating_add(1));
                        cx.notify();
                    });
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &DecreaseDensity, _, cx| {
                    app.state.update(cx, |state, cx| {
                        state.density = DensityTokens::new(state.density.level.saturating_sub(1));
                        cx.notify();
                    });
                    cx.notify();
                }),
            )
            .child(self.workspace.clone())
            .child(self.palette.clone())
    }
}

#[derive(Clone)]
struct SidebarPlaceholder {
    state: Entity<NativeAppState>,
}

impl SidebarPlaceholder {
    fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }
}

impl Render for SidebarPlaceholder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let typography = theme.typography;
        let density = state.density.clone();

        let mut account_list = div().flex().flex_col().gap(px(2.0));
        for account in &state.accounts {
            let folders = state
                .folders_by_account
                .get(&account.id)
                .cloned()
                .unwrap_or_default();
            let account_unread = folders
                .iter()
                .map(|folder| folder.unread_count.max(0))
                .sum();
            account_list = account_list.child(account_tree_header(
                account,
                folders.len(),
                account_unread,
                cx,
            ));

            for folder in folders.iter().filter(|folder| !folder.is_excluded) {
                let folder_id = folder.id.clone();
                let folder_key = folder_id.clone();
                let state_entity = self.state.clone();
                let selected = state.selected_folder.as_deref() == Some(folder.id.as_str())
                    && state.selected_label.is_none();
                account_list = account_list.child(
                    sidebar_nav_row(
                        folder_label(folder),
                        selected,
                        Some(folder.unread_count),
                        folder_indent(folder),
                        cx,
                    )
                    .h(px(density.sidebar_row_height.max(28.0)))
                    .id(SharedString::from(format!("sidebar-folder-{folder_key}")))
                    .cursor_pointer()
                    .on_click(move |_, _, app_cx| {
                        start_message_collection_load(
                            &state_entity,
                            NativeMessageSource::Folder(folder_id.clone()),
                            app_cx,
                        );
                    }),
                );
            }
        }

        if state.accounts.is_empty() {
            account_list = account_list.child(empty_sidebar_text("No accounts configured", cx));
        }

        let unread = total_unread_count(&state);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors.panel_bg)
            .child(
                div()
                    .px(px(14.0))
                    .py(px(12.0))
                    .border_b_1()
                    .border_color(colors.stroke_neutral_subtle)
                    .child(
                        div()
                            .text_size(px(typography.subtitle.size))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.on_neutral)
                            .child("Mail"),
                    )
                    .child(
                        div()
                            .pt(px(2.0))
                            .text_size(px(typography.caption.size))
                            .text_color(colors.on_subtle)
                            .child(unread_status_text(unread)),
                    )
                    .child(
                        div()
                            .mt(px(10.0))
                            .h(px(32.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .bg(colors.accent)
                            .hover({
                                let accent_hover = colors.accent_hover;
                                move |style| style.bg(accent_hover)
                            })
                            .text_size(px(typography.body.size))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.on_accent)
                            .child("Compose"),
                    ),
            )
            .child(sync_status_strip(&state.sync, cx))
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(8.0))
                    .py(px(10.0))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(section_label("Smart folders", cx))
                    .child(smart_folder_rows(&self.state, &state, cx))
                    .child(section_label("Accounts", cx))
                    .child(account_list)
                    .child(section_label("Labels", cx))
                    .child(label_rows(&self.state, &state, cx)),
            )
            .child(
                div()
                    .px(px(8.0))
                    .py(px(10.0))
                    .border_t_1()
                    .border_color(colors.stroke_neutral_subtle)
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        nav_item("Mail", state.selected_view == SelectedView::Mail, cx)
                            .id("nav-mail")
                            .cursor_pointer()
                            .on_click({
                                let state_entity = self.state.clone();
                                move |_, _, app_cx| {
                                    state_entity.update(app_cx, |state, cx| {
                                        state.selected_view = SelectedView::Mail;
                                        cx.notify();
                                    });
                                }
                            }),
                    )
                    .child(
                        nav_item(
                            "Calendar",
                            state.selected_view == SelectedView::Calendar,
                            cx,
                        )
                        .id("nav-calendar")
                        .cursor_pointer()
                        .on_click({
                            let state_entity = self.state.clone();
                            move |_, _, app_cx| {
                                state_entity.update(app_cx, |state, cx| {
                                    state.selected_view = SelectedView::Calendar;
                                    cx.notify();
                                });
                            }
                        }),
                    ),
            )
    }
}

#[derive(Clone)]
struct MessageListPlaceholder {
    state: Entity<NativeAppState>,
}

impl MessageListPlaceholder {
    fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }
}

impl Render for MessageListPlaceholder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let typography = theme.typography;
        let density = state.density.clone();
        let message_items = message_list_items(&state.messages);
        let selected_message = state
            .selected_message
            .clone()
            .or_else(|| state.messages.first().map(|message| message.id.clone()));
        let mailbox_title = current_mailbox_title(&state);
        let message_count = state.messages.len();
        let unread_count = state
            .messages
            .iter()
            .filter(|message| !message.is_read)
            .count();
        let mailbox_summary = if state.messages_resource.is_loading() {
            format!("{message_count} cached messages - loading")
        } else if let Some(error) = state.messages_resource.error() {
            format!("{message_count} cached messages - {error}")
        } else {
            format!("{message_count} messages - {unread_count} unread")
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors.surface)
            .child(
                div()
                    .px(px(18.0))
                    .pt(px(14.0))
                    .pb(px(12.0))
                    .border_b_1()
                    .border_color(colors.stroke_neutral_subtle)
                    .child(
                        div()
                            .text_size(px(typography.subtitle.size))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.on_neutral)
                            .child(mailbox_title),
                    )
                    .child(
                        div()
                            .pt(px(4.0))
                            .text_size(px(typography.caption.size))
                            .text_color(if state.messages_resource.is_error() {
                                colors.status_error
                            } else {
                                colors.on_subtle
                            })
                            .child(mailbox_summary),
                    ),
            )
            .child(
                div()
                    .h(px(28.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .border_b_1()
                    .border_color(colors.stroke_neutral_subtle)
                    .bg(colors.panel_bg)
                    .text_size(px(typography.caption.size))
                    .text_color(colors.on_subtle)
                    .child(div().w(px(112.0)).child("From"))
                    .child(div().flex_1().min_w_0().child("Subject"))
                    .child(div().w(px(90.0)).flex().justify_end().child("Date")),
            )
            .child(
                div().flex_1().min_h_0().child(
                    SelectableList::new("native-message-list")
                        .items(message_items)
                        .empty_text("No messages loaded for the selected folder")
                        .row_height(density.message_row_height.max(58.0))
                        .on_select({
                            let state_entity = self.state.clone();
                            move |message_id, _, app_cx| {
                                start_full_message_load(&state_entity, message_id.as_ref(), app_cx);
                            }
                        })
                        .on_activate({
                            let state_entity = self.state.clone();
                            move |message_id, _, _, app_cx| {
                                start_full_message_load(&state_entity, message_id.as_ref(), app_cx);
                            }
                        })
                        .when_some(selected_message, |list, selected| list.selected(selected)),
                ),
            )
    }
}

#[derive(Clone)]
struct ReadingPanePlaceholder {
    state: Entity<NativeAppState>,
    recipient_input: Entity<TokenInput>,
    snooze_picker: Entity<DateTimePicker>,
}

impl ReadingPanePlaceholder {
    fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let suggestions = recipient_suggestions(state.read(cx));
        let recipient_input = cx.new(move |_| {
            TokenInput::new()
                .placeholder("Add recipient")
                .suggestions(suggestions.clone())
                .allow_freeform(true)
        });
        let default_snooze = default_snooze_value();
        let snooze_picker = cx.new(move |_| {
            DateTimePicker::new()
                .placeholder("Select local time")
                .default_value(default_snooze)
        });
        Self {
            state,
            recipient_input,
            snooze_picker,
        }
    }
}

impl Render for ReadingPanePlaceholder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        reading_placeholder(
            &state,
            self.recipient_input.clone(),
            self.snooze_picker.clone(),
            cx,
        )
    }
}

#[derive(Clone)]
struct StatusBar {
    state: Entity<NativeAppState>,
}

impl StatusBar {
    fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let typography = theme.typography;
        let status = state
            .last_error
            .clone()
            .unwrap_or_else(|| "Axum in-process server ready".to_string());
        let unread = total_unread_count(&state);
        let body_status = if state.selected_message_resource.is_loading() {
            "body loading".to_string()
        } else if state.selected_message_resource.is_error() {
            "body error".to_string()
        } else if state.selected_message_resource.is_ready() {
            "body ready".to_string()
        } else {
            "body idle".to_string()
        };

        div()
            .h(px(28.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .border_t_1()
            .border_color(colors.stroke_neutral_subtle)
            .bg(colors.panel_bg)
            .text_size(px(typography.caption.size))
            .text_color(if state.last_error.is_some() {
                colors.status_error
            } else {
                colors.on_subtle
            })
            .child(format!(
                "{} | {} | {} | {} | density {} | {:?}",
                status,
                resource_summary(&state.messages_resource, "messages"),
                body_status,
                unread_status_text(unread),
                state.density.level,
                state.theme
            ))
    }
}

fn resource_summary<T>(resource: &AsyncResource<Vec<T>, String>, label: &str) -> String {
    let count = resource.value().map(Vec::len).unwrap_or_default();
    if resource.is_loading() {
        format!("{label} loading ({count} cached)")
    } else if let Some(error) = resource.error() {
        if count > 0 {
            format!("{label} error ({count} cached): {error}")
        } else {
            format!("{label} error: {error}")
        }
    } else if resource.is_ready() {
        format!("{count} {label} ready")
    } else {
        format!("{label} idle")
    }
}

fn total_unread_count(state: &NativeAppState) -> i64 {
    state
        .folders_by_account
        .values()
        .flat_map(|folders| folders.iter())
        .map(|folder| folder.unread_count.max(0))
        .sum()
}

fn native_window_title(state: &NativeAppState) -> String {
    let unread = total_unread_count(state);
    if unread > 0 {
        format!("({unread}) {APP_TITLE}")
    } else {
        APP_TITLE.to_string()
    }
}

fn native_window_status(state: &NativeAppState) -> String {
    unread_status_text(total_unread_count(state))
}

fn unread_status_text(unread: i64) -> String {
    match unread {
        0 => "No unread messages".to_string(),
        1 => "1 unread message".to_string(),
        unread => format!("{unread} unread messages"),
    }
}

fn apply_sync_update(status: &mut NativeSyncStatus, update: NativeSyncUpdate) {
    match update {
        NativeSyncUpdate::Connecting => {
            status.connected = false;
            status.last_error = None;
            status.last_message = Some("Connecting to sync events".to_string());
        }
        NativeSyncUpdate::Connected => {
            status.connected = true;
            status.last_error = None;
            status.last_message = Some("Waiting for sync activity".to_string());
        }
        NativeSyncUpdate::Event(SyncEvent::SyncStart {
            account_id,
            folder_count,
        }) => {
            status.connected = true;
            status.syncing = true;
            status.account_id = Some(account_id);
            status.folder = None;
            status.done = 0;
            status.total = folder_count.unwrap_or_default();
            status.last_error = None;
            status.last_message = Some("Sync started".to_string());
        }
        NativeSyncUpdate::Event(SyncEvent::SyncFolderDone {
            account_id,
            folder,
            done,
            total,
        }) => {
            status.connected = true;
            status.syncing = true;
            status.account_id = Some(account_id);
            status.folder = folder;
            status.done = done.unwrap_or(status.done);
            status.total = total.unwrap_or(status.total);
            status.last_error = None;
            status.last_message = Some("Syncing folders".to_string());
        }
        NativeSyncUpdate::Event(SyncEvent::SyncComplete { account_id }) => {
            status.connected = true;
            status.syncing = false;
            status.account_id = Some(account_id);
            status.folder = None;
            status.done = status.total.max(status.done);
            status.last_error = None;
            status.last_message = Some("Sync complete".to_string());
        }
        NativeSyncUpdate::Error(message) => {
            status.connected = false;
            status.syncing = false;
            status.last_error = Some(message);
        }
    }
}

fn sync_status_title(status: &NativeSyncStatus) -> String {
    if let Some(error) = status.last_error.as_deref() {
        return format!("Sync events offline: {error}");
    }
    if status.syncing {
        if status.total > 0 {
            return format!("Syncing {} of {}", status.done.max(0), status.total);
        }
        return "Syncing".to_string();
    }
    status
        .last_message
        .clone()
        .unwrap_or_else(|| "Sync events idle".to_string())
}

fn sync_status_detail(status: &NativeSyncStatus) -> String {
    if status.syncing {
        return status
            .folder
            .clone()
            .filter(|folder| !folder.trim().is_empty())
            .unwrap_or_else(|| "Preparing folders".to_string());
    }
    if status.connected {
        "Live sync listener connected".to_string()
    } else {
        "Live sync listener not connected".to_string()
    }
}

fn sync_progress_ratio(status: &NativeSyncStatus) -> f32 {
    if status.total <= 0 {
        return 0.0;
    }
    (status.done.max(0) as f32 / status.total as f32).clamp(0.0, 1.0)
}

fn sync_window_title(state: &NativeAppState, cx: &mut App) {
    WindowTitle::set_title(cx, native_window_title(state));
    WindowTitle::set_status(cx, native_window_status(state));
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NativeMessageSource {
    Folder(String),
    Label(String),
    SmartFolder(String),
}

impl NativeMessageSource {
    fn apply_selection(&self, state: &mut NativeAppState) {
        state.selected_view = SelectedView::Mail;
        match self {
            Self::Folder(folder_id) => {
                state.selected_folder = Some(folder_id.clone());
                state.selected_label = None;
            }
            Self::Label(label_id) => {
                state.selected_folder = None;
                state.selected_label = Some(label_id.clone());
            }
            Self::SmartFolder(kind) => {
                state.selected_folder = Some(format!("smart:{kind}"));
                state.selected_label = None;
            }
        }
    }

    fn load_context(&self) -> String {
        match self {
            Self::Folder(folder_id) => format!("list messages for {folder_id}"),
            Self::Label(label_id) => format!("list messages for label {label_id}"),
            Self::SmartFolder(kind) => format!("list smart folder {kind}"),
        }
    }
}

#[derive(Clone, Debug)]
struct LoadedMessageCollection {
    messages: Vec<Message>,
    selected_full_message: Option<FullMessage>,
}

fn start_message_collection_load(
    state_entity: &Entity<NativeAppState>,
    source: NativeMessageSource,
    app_cx: &mut App,
) {
    let load_source = source.clone();
    let (base_url, request) = state_entity.update(app_cx, |state, cx| {
        let request = begin_message_collection_load(state, &source);
        sync_window_title(state, cx);
        cx.notify();
        (state.api_base_url.clone(), request)
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { load_message_collection(base_url, load_source) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_loaded_message_collection(state, request, loaded);
                        }
                        Err(err) => {
                            reject_message_collection_load(state, request, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native message collection load: {err}");
                }
            }
        })
        .detach();
}

fn begin_message_collection_load(
    state: &mut NativeAppState,
    source: &NativeMessageSource,
) -> ResourceRequest {
    source.apply_selection(state);
    state.messages.clear();
    state.selected_message = None;
    state.selected_full_message = None;
    state.selected_message_resource.reset();
    state.last_error = None;
    state.messages_resource.begin_load()
}

fn load_message_collection(
    base_url: String,
    source: NativeMessageSource,
) -> Result<LoadedMessageCollection> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        let mut messages = match &source {
            NativeMessageSource::Folder(folder_id) => client.list_folder_messages(folder_id).await,
            NativeMessageSource::Label(label_id) => client.list_label_messages(label_id).await,
            NativeMessageSource::SmartFolder(kind) => client.list_smart_messages(kind).await,
        }
        .with_context(|| source.load_context())?;

        let selected_full_message = if let Some(message_id) = messages.first().map(|m| m.id.clone())
        {
            match client.get_message(&message_id).await {
                Ok(full_message) => {
                    update_summary_from_full_message(&mut messages, &full_message);
                    Some(full_message)
                }
                Err(err) => {
                    tracing::warn!("failed to load selected message {message_id}: {err}");
                    None
                }
            }
        } else {
            None
        };

        Ok(LoadedMessageCollection {
            messages,
            selected_full_message,
        })
    })
}

fn apply_loaded_message_collection(
    state: &mut NativeAppState,
    request: ResourceRequest,
    loaded: LoadedMessageCollection,
) -> bool {
    if !state
        .messages_resource
        .resolve(request, loaded.messages.clone())
    {
        return false;
    }

    state.messages = loaded.messages;
    state.selected_message = state.messages.first().map(|message| message.id.clone());
    state.selected_full_message = None;
    state.selected_message_resource.reset();

    if let Some(full_message) = loaded.selected_full_message {
        state.selected_message = Some(full_message.id.clone());
        state.selected_message_resource = AsyncResource::ready(full_message.clone());
        apply_full_message(state, full_message);
    }

    state.last_error = None;
    true
}

fn reject_message_collection_load(
    state: &mut NativeAppState,
    request: ResourceRequest,
    err: anyhow::Error,
) -> bool {
    let message = err.to_string();
    if !state.messages_resource.reject(request, message.clone()) {
        return false;
    }

    state.messages.clear();
    state.selected_message = None;
    state.selected_full_message = None;
    state.selected_message_resource.reset();
    state.last_error = Some(message);
    true
}

fn start_full_message_load(
    state_entity: &Entity<NativeAppState>,
    message_id: &str,
    app_cx: &mut App,
) {
    let message_id = message_id.to_string();
    let Some((base_url, request)) = state_entity.update(app_cx, |state, cx| {
        let Some(request) = begin_full_message_load(state, &message_id) else {
            return None;
        };
        sync_window_title(state, cx);
        cx.notify();
        Some((state.api_base_url.clone(), request))
    }) else {
        return;
    };

    let load_message_id = message_id.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { load_full_message(base_url, load_message_id) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(full_message) => {
                            apply_loaded_full_message(state, request, &message_id, full_message);
                        }
                        Err(err) => {
                            reject_full_message_load(state, request, &message_id, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native message load: {err}");
                }
            }
        })
        .detach();
}

fn begin_full_message_load(
    state: &mut NativeAppState,
    message_id: &str,
) -> Option<ResourceRequest> {
    if state.selected_full_message.as_ref().is_some_and(|message| {
        message.id == message_id && state.selected_message_resource.is_ready()
    }) {
        return None;
    }

    state.selected_message = Some(message_id.to_string());
    state.selected_full_message = None;
    state.last_error = None;
    Some(state.selected_message_resource.begin_load())
}

fn load_full_message(base_url: String, message_id: String) -> Result<FullMessage> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .get_message(&message_id)
            .await
            .with_context(|| format!("get message {message_id}"))
    })
}

fn apply_loaded_full_message(
    state: &mut NativeAppState,
    request: ResourceRequest,
    message_id: &str,
    full_message: FullMessage,
) -> bool {
    if state.selected_message.as_deref() != Some(message_id) {
        return false;
    }
    if !state
        .selected_message_resource
        .resolve(request, full_message.clone())
    {
        return false;
    }

    apply_full_message(state, full_message);
    state.last_error = None;
    true
}

fn reject_full_message_load(
    state: &mut NativeAppState,
    request: ResourceRequest,
    message_id: &str,
    err: anyhow::Error,
) -> bool {
    if state.selected_message.as_deref() != Some(message_id) {
        return false;
    }

    let message = err.to_string();
    if !state
        .selected_message_resource
        .reject(request, message.clone())
    {
        return false;
    }

    state.selected_full_message = None;
    state.last_error = Some(message);
    true
}

fn update_summary_from_full_message(messages: &mut [Message], full_message: &FullMessage) {
    if let Some(summary) = messages
        .iter_mut()
        .find(|message| message.id == full_message.id)
    {
        summary.is_read = full_message.is_read;
        summary.is_flagged = full_message.is_flagged;
    }
}

fn apply_full_message(state: &mut NativeAppState, full_message: FullMessage) {
    let was_unread = state
        .messages
        .iter()
        .find(|message| message.id == full_message.id)
        .is_some_and(|message| !message.is_read);
    update_summary_from_full_message(&mut state.messages, &full_message);
    if was_unread && full_message.is_read {
        decrement_folder_unread(&mut state.folders_by_account, &full_message.folder_id);
    }
    state.selected_message = Some(full_message.id.clone());
    state.selected_full_message = Some(full_message);
}

fn decrement_folder_unread(
    folders_by_account: &mut BTreeMap<String, Vec<Folder>>,
    folder_id: &str,
) {
    for folder in folders_by_account
        .values_mut()
        .flat_map(|folders| folders.iter_mut())
    {
        if folder.id == folder_id {
            folder.unread_count = folder.unread_count.saturating_sub(1);
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: &str, unread_count: i64) -> Folder {
        Folder {
            id: id.to_string(),
            account_id: "account".to_string(),
            name: id.to_string(),
            full_path: id.to_string(),
            special_use: None,
            unread_count,
            total_count: unread_count.max(0),
            synced_at: None,
            is_excluded: false,
        }
    }

    fn message(id: &str, folder_id: &str, is_read: bool) -> Message {
        Message {
            id: id.to_string(),
            account_id: "account".to_string(),
            folder_id: folder_id.to_string(),
            uid: 1,
            message_id: None,
            thread_id: None,
            subject: Some("Subject".to_string()),
            from_name: None,
            from_email: None,
            to_json: None,
            date: None,
            is_read,
            is_flagged: false,
            is_draft: false,
            has_attachments: false,
            preview: None,
            snoozed_until: None,
        }
    }

    fn full_message(id: &str, folder_id: &str, is_read: bool, is_flagged: bool) -> FullMessage {
        FullMessage {
            id: id.to_string(),
            account_id: "account".to_string(),
            folder_id: folder_id.to_string(),
            uid: 1,
            message_id: None,
            thread_id: None,
            subject: Some("Subject".to_string()),
            from_name: None,
            from_email: None,
            to: None,
            date: None,
            is_read,
            is_flagged,
            is_draft: false,
            has_attachments: false,
            preview: None,
            body: None,
        }
    }

    #[test]
    fn native_window_title_uses_folder_unread_counts() {
        let mut state = NativeAppState::default();
        state.folders_by_account.insert(
            "account".to_string(),
            vec![folder("inbox", 2), folder("archive", 1), folder("bad", -4)],
        );

        assert_eq!(total_unread_count(&state), 3);
        assert_eq!(native_window_title(&state), "(3) email-rs FluentGUI");
        assert_eq!(native_window_status(&state), "3 unread messages");
    }

    #[test]
    fn native_window_title_omits_zero_unread_prefix() {
        let state = NativeAppState::default();

        assert_eq!(native_window_title(&state), "email-rs FluentGUI");
        assert_eq!(native_window_status(&state), "No unread messages");
    }

    #[test]
    fn sync_updates_track_folder_progress() {
        let mut status = NativeSyncStatus::default();

        apply_sync_update(&mut status, NativeSyncUpdate::Connected);
        assert!(status.connected);
        assert_eq!(sync_status_title(&status), "Waiting for sync activity");

        apply_sync_update(
            &mut status,
            NativeSyncUpdate::Event(SyncEvent::SyncStart {
                account_id: "account".to_string(),
                folder_count: Some(4),
            }),
        );
        assert!(status.syncing);
        assert_eq!(status.total, 4);
        assert_eq!(sync_progress_ratio(&status), 0.0);

        apply_sync_update(
            &mut status,
            NativeSyncUpdate::Event(SyncEvent::SyncFolderDone {
                account_id: "account".to_string(),
                folder: Some("INBOX".to_string()),
                done: Some(2),
                total: Some(4),
            }),
        );
        assert_eq!(status.folder.as_deref(), Some("INBOX"));
        assert_eq!(sync_progress_ratio(&status), 0.5);

        apply_sync_update(
            &mut status,
            NativeSyncUpdate::Event(SyncEvent::SyncComplete {
                account_id: "account".to_string(),
            }),
        );
        assert!(!status.syncing);
        assert_eq!(status.done, 4);
        assert_eq!(sync_status_title(&status), "Sync complete");
    }

    #[test]
    fn sse_parser_handles_split_sync_events() {
        let mut buffer = String::new();
        let first = drain_sse_sync_events(
            &mut buffer,
            "event: sync\ndata: {\"type\":\"sync_start\",\"accountId\":\"a",
        );
        assert!(first.is_empty());

        let events = drain_sse_sync_events(
            &mut buffer,
            "1\",\"folderCount\":3}\n\nevent: sync\ndata: {\"type\":\"sync_complete\",\"accountId\":\"a1\"}\n\n",
        );

        assert_eq!(
            events,
            vec![
                Ok(SyncEvent::SyncStart {
                    account_id: "a1".to_string(),
                    folder_count: Some(3),
                }),
                Ok(SyncEvent::SyncComplete {
                    account_id: "a1".to_string(),
                }),
            ]
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn sse_parser_handles_crlf_event_boundaries() {
        let mut buffer = String::new();
        let events = drain_sse_sync_events(
            &mut buffer,
            "event: sync\r\ndata: {\"type\":\"sync_folder_done\",\"accountId\":\"a1\",\"folder\":\"INBOX\",\"done\":2,\"total\":5}\r\n\r\n",
        );

        assert_eq!(
            events,
            vec![Ok(SyncEvent::SyncFolderDone {
                account_id: "a1".to_string(),
                folder: Some("INBOX".to_string()),
                done: Some(2),
                total: Some(5),
            })]
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn applying_read_full_message_updates_summary_and_folder_unread() {
        let mut state = NativeAppState::default();
        state
            .folders_by_account
            .insert("account".to_string(), vec![folder("inbox", 1)]);
        state.messages.push(message("message", "inbox", false));

        apply_full_message(&mut state, full_message("message", "inbox", true, true));

        assert!(state.messages[0].is_read);
        assert!(state.messages[0].is_flagged);
        assert_eq!(total_unread_count(&state), 0);
    }

    #[test]
    fn beginning_collection_load_sets_route_and_loading_state() {
        let mut state = NativeAppState::default();
        state.messages.push(message("old", "inbox", false));
        state.selected_message = Some("old".to_string());
        state.selected_full_message = Some(full_message("old", "inbox", false, false));

        let request = begin_message_collection_load(
            &mut state,
            &NativeMessageSource::Label("label-a".to_string()),
        );

        assert_eq!(state.selected_view, SelectedView::Mail);
        assert_eq!(state.selected_folder, None);
        assert_eq!(state.selected_label.as_deref(), Some("label-a"));
        assert!(state.messages.is_empty());
        assert_eq!(state.selected_message, None);
        assert!(state.selected_full_message.is_none());
        assert!(state.selected_message_resource.is_idle());
        assert_eq!(state.messages_resource.current_request(), Some(request));
        assert!(state.messages_resource.is_loading());
    }

    #[test]
    fn stale_collection_load_is_ignored() {
        let mut state = NativeAppState::default();
        let stale = begin_message_collection_load(
            &mut state,
            &NativeMessageSource::Folder("inbox".to_string()),
        );
        let current = begin_message_collection_load(
            &mut state,
            &NativeMessageSource::Folder("archive".to_string()),
        );

        let loaded = LoadedMessageCollection {
            messages: vec![message("stale", "inbox", false)],
            selected_full_message: None,
        };

        assert!(!apply_loaded_message_collection(&mut state, stale, loaded));
        assert_eq!(state.selected_folder.as_deref(), Some("archive"));
        assert!(state.messages.is_empty());
        assert_eq!(state.messages_resource.current_request(), Some(current));
        assert!(state.messages_resource.is_loading());
    }

    #[test]
    fn current_collection_load_applies_first_full_message() {
        let mut state = NativeAppState::default();
        state
            .folders_by_account
            .insert("account".to_string(), vec![folder("inbox", 1)]);
        let request = begin_message_collection_load(
            &mut state,
            &NativeMessageSource::Folder("inbox".to_string()),
        );
        let loaded = LoadedMessageCollection {
            messages: vec![message("message", "inbox", false)],
            selected_full_message: Some(full_message("message", "inbox", true, true)),
        };

        assert!(apply_loaded_message_collection(&mut state, request, loaded));
        assert_eq!(state.selected_message.as_deref(), Some("message"));
        assert!(state.selected_full_message.is_some());
        assert!(state.selected_message_resource.is_ready());
        assert!(state.messages[0].is_read);
        assert!(state.messages[0].is_flagged);
        assert_eq!(total_unread_count(&state), 0);
    }

    #[test]
    fn stale_full_message_load_is_ignored() {
        let mut state = NativeAppState::default();
        let stale = begin_full_message_load(&mut state, "first").expect("first request");
        let current = begin_full_message_load(&mut state, "second").expect("second request");

        assert!(!apply_loaded_full_message(
            &mut state,
            stale,
            "first",
            full_message("first", "inbox", true, false),
        ));
        assert_eq!(state.selected_message.as_deref(), Some("second"));
        assert_eq!(
            state.selected_message_resource.current_request(),
            Some(current)
        );
        assert!(state.selected_message_resource.is_loading());
        assert!(state.selected_full_message.is_none());
    }
}

fn palette_entries(state: &NativeAppState) -> Vec<PaletteEntry> {
    let mut entries = vec![
        PaletteEntry::new("compose", "New Message")
            .group("Actions")
            .hint("c")
            .subtitle("Compose editor lands in Phase 5")
            .keyword("compose")
            .keyword("write")
            .disabled(true),
        PaletteEntry::new("advanced-search", "Advanced Search")
            .group("Actions")
            .subtitle("Advanced search modal lands in Phase 4")
            .keyword("search")
            .keyword("filter")
            .disabled(true),
        PaletteEntry::new("settings", "Open Settings")
            .group("Actions")
            .subtitle("Settings modal lands in Phase 6")
            .keyword("preferences")
            .keyword("options")
            .disabled(true),
        PaletteEntry::new("theme-light", "Light theme")
            .group("Actions")
            .hint("Ctrl+Shift+L")
            .subtitle("Apply Fluent light theme")
            .keyword("appearance")
            .action(|_, cx| cx.dispatch_action(&ApplyLightTheme)),
        PaletteEntry::new("theme-dark", "Dark theme")
            .group("Actions")
            .hint("Ctrl+Shift+D")
            .subtitle("Apply Fluent dark theme")
            .keyword("appearance")
            .action(|_, cx| cx.dispatch_action(&ApplyDarkTheme)),
        PaletteEntry::new("density-down", "Density: Denser")
            .group("Actions")
            .hint("Ctrl+-")
            .keyword("compact")
            .keyword("layout")
            .disabled(state.density.level == 0)
            .action(|_, cx| cx.dispatch_action(&DecreaseDensity)),
        PaletteEntry::new("density-up", "Density: Spacious")
            .group("Actions")
            .hint("Ctrl++")
            .keyword("spacious")
            .keyword("layout")
            .disabled(state.density.level >= 8)
            .action(|_, cx| cx.dispatch_action(&IncreaseDensity)),
        PaletteEntry::new("view-mail", "Go to Mail")
            .group("Navigate")
            .subtitle("Open the mail workspace")
            .keyword("inbox")
            .action(|_, cx| cx.dispatch_action(&SelectMailView)),
        PaletteEntry::new("view-calendar", "Go to Calendar")
            .group("Navigate")
            .subtitle("Open the calendar workspace")
            .keyword("events")
            .keyword("schedule")
            .action(|_, cx| cx.dispatch_action(&SelectCalendarView)),
    ];

    entries.extend(state.labels.iter().map(|label| {
        PaletteEntry::new(format!("label:{}", label.id), label.name.clone())
            .group("Labels")
            .hint("label")
            .subtitle("Label navigation lands in Phase 4")
            .keyword("label")
            .keyword("tag")
            .disabled(true)
    }));

    entries.extend(state.folders_by_account.iter().flat_map(|(_, folders)| {
        folders
            .iter()
            .filter(|folder| !folder.is_excluded)
            .map(|folder| {
                PaletteEntry::new(format!("folder:{}", folder.id), folder.name.clone())
                    .group("Folders")
                    .hint(folder.full_path.clone())
                    .subtitle("Folder navigation lands in Phase 4")
                    .keyword("folder")
                    .keyword(folder.full_path.clone())
                    .disabled(true)
            })
    }));

    entries.extend(state.accounts.iter().map(|account| {
        PaletteEntry::new(account.id.clone(), account.name.clone())
            .group("Accounts")
            .subtitle(account.email.clone())
            .keyword("account")
            .disabled(true)
    }));

    entries
}

fn section_label(label: &'static str, cx: &mut Context<SidebarPlaceholder>) -> gpui::Div {
    let theme = cx.theme();
    div()
        .px(px(8.0))
        .pt(px(6.0))
        .pb(px(2.0))
        .text_size(px(theme.typography.caption.size))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.colors.on_subtle)
        .child(label)
}

fn account_tree_header(
    account: &Account,
    folder_count: usize,
    unread_count: i64,
    cx: &mut Context<SidebarPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let summary = if unread_count > 0 {
        format!("{folder_count} folders, {unread_count} unread")
    } else {
        format!("{folder_count} folders")
    };

    div()
        .h(px(42.0))
        .pl(px(8.0))
        .pr(px(6.0))
        .flex()
        .items_center()
        .gap(px(7.0))
        .child(
            div()
                .w(px(10.0))
                .text_size(px(typography.caption.size))
                .text_color(colors.on_subtle)
                .child("v"),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(typography.body.size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.on_neutral)
                        .child(account.name.clone()),
                )
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(typography.caption.size))
                        .text_color(colors.on_subtle)
                        .child(summary),
                ),
        )
}

fn empty_sidebar_text(message: &'static str, cx: &mut Context<SidebarPlaceholder>) -> gpui::Div {
    let theme = cx.theme();
    div()
        .px(px(8.0))
        .py(px(6.0))
        .text_size(px(theme.typography.body.size))
        .text_color(theme.colors.on_subtle)
        .child(message)
}

fn sync_status_strip(status: &NativeSyncStatus, cx: &mut Context<SidebarPlaceholder>) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let ratio = sync_progress_ratio(status);

    div()
        .mx(px(8.0))
        .mt(px(8.0))
        .mb(px(2.0))
        .px(px(8.0))
        .py(px(7.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(colors.stroke_neutral_subtle)
        .bg(colors.surface)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(typography.caption.size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if status.last_error.is_some() {
                            colors.status_error
                        } else {
                            colors.on_neutral
                        })
                        .child(sync_status_title(status)),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(10.0))
                        .text_color(colors.on_subtle)
                        .child(if status.connected { "Live" } else { "Idle" }),
                ),
        )
        .child(
            div()
                .pt(px(3.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(typography.caption.size))
                .text_color(colors.on_subtle)
                .child(sync_status_detail(status)),
        )
        .when(status.syncing, |strip| {
            strip.child(
                div()
                    .mt(px(6.0))
                    .w_full()
                    .child(ProgressBar::new(ratio).height(3.0)),
            )
        })
}

fn smart_folder_rows(
    state_entity: &Entity<NativeAppState>,
    state: &NativeAppState,
    cx: &mut Context<SidebarPlaceholder>,
) -> gpui::Div {
    let mut rows = div().flex().flex_col().gap(px(1.0));
    let unread = total_unread_count(state);
    for (kind, label) in [
        ("all", "All Inboxes"),
        ("unread", "Unread"),
        ("flagged", "Flagged"),
        ("snoozed", "Snoozed"),
    ] {
        let smart_id = format!("smart:{kind}");
        let selected = state.selected_folder.as_deref() == Some(smart_id.as_str());
        let kind = kind.to_string();
        let smart_key = kind.clone();
        let state_entity = state_entity.clone();
        let count = if kind == "all" || kind == "unread" {
            Some(unread)
        } else {
            None
        };
        rows = rows.child(
            sidebar_nav_row(label.to_string(), selected, count, 0, cx)
                .id(SharedString::from(format!("smart-folder-{smart_key}")))
                .cursor_pointer()
                .on_click(move |_, _, app_cx| {
                    start_message_collection_load(
                        &state_entity,
                        NativeMessageSource::SmartFolder(kind.clone()),
                        app_cx,
                    );
                }),
        );
    }
    rows
}

fn label_rows(
    state_entity: &Entity<NativeAppState>,
    state: &NativeAppState,
    cx: &mut Context<SidebarPlaceholder>,
) -> gpui::Div {
    if state.labels.is_empty() {
        return empty_sidebar_text("No labels loaded", cx);
    }

    let mut rows = div().flex().flex_col().gap(px(1.0));
    for label in &state.labels {
        let label_id = label.id.clone();
        let label_key = label_id.clone();
        let selected = state.selected_label.as_deref() == Some(label.id.as_str());
        let state_entity = state_entity.clone();
        rows = rows.child(
            sidebar_nav_row(label.name.clone(), selected, None, 0, cx)
                .id(SharedString::from(format!("label-{label_key}")))
                .cursor_pointer()
                .on_click(move |_, _, app_cx| {
                    start_message_collection_load(
                        &state_entity,
                        NativeMessageSource::Label(label_id.clone()),
                        app_cx,
                    );
                }),
        );
    }
    rows
}

fn sidebar_nav_row(
    label: String,
    selected: bool,
    count: Option<i64>,
    indent: usize,
    cx: &mut Context<SidebarPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let left_padding = 8.0 + indent.min(4) as f32 * 14.0;
    let mut row = div()
        .min_h(px(28.0))
        .pl(px(left_padding))
        .pr(px(8.0))
        .flex()
        .items_center()
        .justify_between()
        .rounded(px(3.0))
        .bg(if selected {
            colors.neutral_selected
        } else {
            colors.panel_bg
        })
        .text_size(px(typography.body.size))
        .text_color(colors.on_neutral)
        .hover({
            let hover = colors.neutral_hover;
            let selected_bg = colors.neutral_selected;
            move |style| style.bg(if selected { selected_bg } else { hover })
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .min_w_0()
                .child(
                    div()
                        .w(px(3.0))
                        .h(px(18.0))
                        .rounded(px(999.0))
                        .bg(if selected {
                            colors.accent
                        } else {
                            colors.panel_bg
                        }),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label),
                ),
        );

    if let Some(count) = count.filter(|count| *count > 0) {
        row = row.child(
            div()
                .flex_none()
                .min_w(px(18.0))
                .text_size(px(typography.caption.size))
                .text_color(colors.on_subtle)
                .child(count.to_string()),
        );
    }

    row
}

fn folder_label(folder: &Folder) -> String {
    if folder.full_path == folder.name || folder.full_path.trim().is_empty() {
        folder.name.clone()
    } else {
        folder
            .full_path
            .rsplit('/')
            .next()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(folder.name.as_str())
            .to_string()
    }
}

fn folder_indent(folder: &Folder) -> usize {
    1 + folder.full_path.matches('/').count().min(3)
}

fn nav_item(
    label: &'static str,
    selected: bool,
    cx: &mut Context<SidebarPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    div()
        .h(px(30.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .rounded(px(3.0))
        .bg(if selected {
            colors.neutral_selected
        } else {
            colors.panel_bg
        })
        .hover({
            let hover = colors.neutral_hover;
            let selected_bg = colors.neutral_selected;
            move |style| style.bg(if selected { selected_bg } else { hover })
        })
        .text_color(colors.on_neutral)
        .child(
            div()
                .w(px(3.0))
                .h(px(18.0))
                .rounded(px(999.0))
                .bg(if selected {
                    colors.accent
                } else {
                    colors.panel_bg
                }),
        )
        .child(label)
}

fn message_list_items(messages: &[Message]) -> Vec<SelectableListItem> {
    messages
        .iter()
        .map(|message| {
            let title = message
                .subject
                .clone()
                .unwrap_or_else(|| "(no subject)".to_string());
            let sender = message
                .from_name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .or_else(|| message.from_email.clone())
                .unwrap_or_else(|| "Unknown sender".to_string());
            let mut item = SelectableListItem::new(message.id.clone(), title)
                .subtitle(sender)
                .detail(message.preview.clone().unwrap_or_default())
                .emphasized(!message.is_read);

            if let Some(date) = message.date.clone().filter(|date| !date.trim().is_empty()) {
                item = item.meta(date);
            }

            if message.is_flagged {
                item = item.badge("Flagged");
            } else if message.has_attachments {
                item = item.badge("Attachment");
            } else if message.snoozed_until.is_some() {
                item = item.badge("Snoozed");
            }

            item
        })
        .collect()
}

fn current_mailbox_title(state: &NativeAppState) -> String {
    if state.selected_view == SelectedView::Calendar {
        return "Calendar".to_string();
    }

    if let Some(label_id) = state.selected_label.as_deref() {
        return state
            .labels
            .iter()
            .find(|label| label.id == label_id)
            .map(|label| label.name.clone())
            .unwrap_or_else(|| "Label".to_string());
    }

    if let Some(folder_id) = state.selected_folder.as_deref() {
        if let Some(kind) = folder_id.strip_prefix("smart:") {
            return smart_folder_title(kind).to_string();
        }

        if let Some(folder) = state
            .folders_by_account
            .values()
            .flat_map(|folders| folders.iter())
            .find(|folder| folder.id == folder_id)
        {
            return folder_label(folder);
        }
    }

    "Mail".to_string()
}

fn smart_folder_title(kind: &str) -> &'static str {
    match kind {
        "all" => "All Inboxes",
        "unread" => "Unread",
        "flagged" => "Flagged",
        "snoozed" => "Snoozed",
        _ => "Smart Folder",
    }
}

fn selected_summary(state: &NativeAppState) -> Option<&Message> {
    let selected_id = state.selected_message.as_deref()?;
    state
        .messages
        .iter()
        .find(|message| message.id == selected_id)
}

fn selected_subject(state: &NativeAppState) -> String {
    state
        .selected_full_message
        .as_ref()
        .and_then(|message| message.subject.clone())
        .or_else(|| selected_summary(state).and_then(|message| message.subject.clone()))
        .filter(|subject| !subject.trim().is_empty())
        .unwrap_or_else(|| "(no subject)".to_string())
}

fn selected_sender(state: &NativeAppState) -> String {
    state
        .selected_full_message
        .as_ref()
        .and_then(|message| {
            message
                .from_name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .or_else(|| message.from_email.clone())
        })
        .or_else(|| {
            selected_summary(state).and_then(|message| {
                message
                    .from_name
                    .clone()
                    .filter(|name| !name.trim().is_empty())
                    .or_else(|| message.from_email.clone())
            })
        })
        .unwrap_or_else(|| "Unknown sender".to_string())
}

fn selected_date(state: &NativeAppState) -> Option<String> {
    state
        .selected_full_message
        .as_ref()
        .and_then(|message| message.date.clone())
        .or_else(|| selected_summary(state).and_then(|message| message.date.clone()))
        .filter(|date| !date.trim().is_empty())
}

fn reading_toolbar_button(
    label: &'static str,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    div()
        .h(px(28.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .border_1()
        .border_color(theme.colors.stroke_neutral_subtle)
        .text_size(px(theme.typography.caption.size))
        .text_color(theme.colors.on_neutral)
        .child(label)
}

fn reading_badge(label: &'static str, cx: &mut Context<ReadingPanePlaceholder>) -> gpui::Div {
    let theme = cx.theme();
    div()
        .h(px(24.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .rounded(px(999.0))
        .bg(theme.colors.neutral)
        .text_size(px(theme.typography.caption.size))
        .text_color(theme.colors.on_subtle)
        .child(label)
}

fn reading_placeholder(
    state: &NativeAppState,
    _recipient_input: Entity<TokenInput>,
    snooze_picker: Entity<DateTimePicker>,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let selected_id = state.selected_message.clone();
    let body_status = if state.selected_message_resource.is_loading() {
        Some("Loading message body...".to_string())
    } else {
        state
            .selected_message_resource
            .error()
            .map(|error| format!("Message body failed to load: {error}"))
    };

    if selected_id.is_none() {
        return div()
            .size_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(colors.stroke_neutral_subtle)
            .bg(colors.surface)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w(px(320.0))
                            .text_size(px(typography.body.size))
                            .text_color(colors.on_subtle)
                            .child("Select a message to read it."),
                    ),
            );
    }

    let subject = selected_subject(state);
    let sender = selected_sender(state);
    let date = selected_date(state).unwrap_or_else(|| "No date".to_string());
    let summary = selected_summary(state);
    let is_flagged = state
        .selected_full_message
        .as_ref()
        .map(|message| message.is_flagged)
        .or_else(|| summary.map(|message| message.is_flagged))
        .unwrap_or(false);
    let has_attachments = state
        .selected_full_message
        .as_ref()
        .map(|message| message.has_attachments)
        .or_else(|| summary.map(|message| message.has_attachments))
        .unwrap_or(false);

    div()
        .size_full()
        .flex()
        .flex_col()
        .border_l_1()
        .border_color(colors.stroke_neutral_subtle)
        .bg(colors.surface)
        .child(
            div()
                .px(px(20.0))
                .py(px(14.0))
                .border_b_1()
                .border_color(colors.stroke_neutral_subtle)
                .child(
                    div()
                        .text_size(px(typography.subtitle.size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.on_neutral)
                        .child(subject),
                )
                .child(
                    div()
                        .pt(px(5.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(typography.body.size))
                                .text_color(colors.on_subtle)
                                .child(sender),
                        )
                        .child(
                            div()
                                .text_size(px(typography.caption.size))
                                .text_color(colors.on_subtle)
                                .child(date),
                        ),
                )
                .child(
                    div()
                        .pt(px(10.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(reading_toolbar_button("Reply", cx))
                        .child(reading_toolbar_button("Forward", cx))
                        .child(reading_toolbar_button("Archive", cx))
                        .child(reading_toolbar_button("Delete", cx))
                        .child(reading_toolbar_button("Snooze", cx))
                        .child(div().w(px(190.0)).child(snooze_picker))
                        .when(is_flagged, |row| row.child(reading_badge("Flagged", cx)))
                        .when(has_attachments, |row| {
                            row.child(reading_badge("Attachment", cx))
                        }),
                ),
        )
        .when_some(body_status, |pane, status| {
            pane.child(
                div()
                    .mx(px(20.0))
                    .mt(px(12.0))
                    .p(px(8.0))
                    .rounded(px(4.0))
                    .bg(colors.neutral)
                    .text_size(px(typography.caption.size))
                    .text_color(colors.on_subtle)
                    .child(status),
            )
        })
        .child(
            div()
                .flex_1()
                .min_h_0()
                .px(px(20.0))
                .py(px(16.0))
                .child(DocumentViewer::new(selected_document(state)).max_height(520.0)),
        )
}

fn selected_document(state: &NativeAppState) -> RichDocument {
    if let Some(message) = state.selected_full_message.as_ref() {
        if let Some(body) = message.body.as_ref() {
            if let Some(html) = body
                .html_body
                .as_ref()
                .filter(|html| !html.trim().is_empty())
            {
                return RichDocument::from_html_fragment_lossy(html);
            }
            if let Some(text) = body
                .text_body
                .as_ref()
                .filter(|text| !text.trim().is_empty())
            {
                return RichDocument::from_plain_text(text);
            }
        }
        return RichDocument::from_plain_text(
            message
                .preview
                .as_deref()
                .unwrap_or("No message body was loaded."),
        );
    }

    let preview = state
        .selected_message
        .as_deref()
        .and_then(|id| state.messages.iter().find(|message| message.id == id))
        .and_then(|message| message.preview.as_deref())
        .unwrap_or("Select a message to load its body.");
    RichDocument::from_plain_text(preview)
}

fn default_snooze_value() -> DateTimeValue {
    let tomorrow = Local::now() + ChronoDuration::days(1);
    DateTimeValue::new(
        tomorrow.year(),
        tomorrow.month() as u8,
        tomorrow.day() as u8,
        9,
        0,
    )
    .unwrap_or(DateTimeValue {
        year: 2000,
        month: 1,
        day: 1,
        hour: 9,
        minute: 0,
    })
}

fn recipient_suggestions(state: &NativeAppState) -> Vec<TokenInputSuggestion> {
    let mut seen = BTreeSet::new();
    let mut suggestions = Vec::new();

    for message in &state.messages {
        let Some(email) = message.from_email.as_deref().map(str::trim) else {
            continue;
        };
        if email.is_empty() || !seen.insert(email.to_lowercase()) {
            continue;
        }

        let label = message
            .from_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(email)
            .to_string();

        let mut suggestion = TokenInputSuggestion::new(email.to_string(), label.clone());
        if label != email {
            suggestion = suggestion.detail(email.to_string());
        }
        suggestions.push(suggestion);
    }

    suggestions
}
