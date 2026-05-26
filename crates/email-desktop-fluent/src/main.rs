use std::collections::{BTreeMap, BTreeSet};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context as _, Result};
use chrono::{Datelike as _, Duration as ChronoDuration, Local, TimeZone as _, Utc};
use email_client::EmailClient;
use email_server::{run_server, Config};
use email_types::{
    Account, AccountSettings, ActionInput, BulkAction, BulkRequest, Condition, ConditionField,
    ConditionGroup, ConditionInput, ConditionOperator, CreateAccount, CreateWebhook, Folder,
    FullMessage, Label, MatchMode, Message, PatchFolder, PatchMessageRequest, RuleDto, SendRequest,
    SnoozeRequest, SyncEvent, UpdateAccount, UpdateRuleBody, UpdateWebhook, Webhook,
};
use fluent_app::{FluentApp, WindowTitle};
use fluent_core::{AsyncResource, ResourceRequest, Theme, ThemeProvider as _};
use fluent_layout::{
    CommandPalette, ContextMenu, DockPanel, DockPosition, DocumentViewer, ModalHost, PaletteEntry,
    Pane, PaneConstraints, PaneGroup, RichDocument, Workspace,
};
use fluent_primitives::{
    Button, ButtonAppearance, ButtonSize, Checkbox, CheckboxState, DateTimePicker, DateTimeValue,
    Dropdown, IconButton, ProgressBar, Searchbox, TextInput, Textarea, TokenInput,
    TokenInputSuggestion, TokenInputToken,
};
use gpui::{
    actions, div, prelude::*, px, svg, uniform_list, App, Context, Entity, FocusHandle, FontWeight,
    IntoElement, KeyBinding, KeyDownEvent, MouseButton, MouseDownEvent, Pixels, Point, Render,
    SharedString, Window,
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
        DecreaseDensity,
        StartCompose,
        StartReplyCompose,
        StartForwardCompose,
        OpenAdvancedSearch,
        OpenAccountSetup,
        OpenSettings
    ]
);

const APP_TITLE: &str = "email-rs FluentGUI";
const APP_ID: &str = "org.email-rs.desktop.fluent";
const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(10);
const NATIVE_SETTINGS_ENV: &str = "EMAIL_RS_FLUENT_SETTINGS_PATH";
const NATIVE_SETTINGS_FILE: &str = "fluentgui-settings.conf";

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
    let mut initial_state = load_initial_state(&base_url)?;
    let preferences = load_native_preferences();
    preferences.apply_to_state(&mut initial_state);

    let fluent_app = FluentApp::new(native_window_title(&initial_state))
        .app_id(APP_ID)
        .window_size(1280.0, 800.0)
        .window_min_size(800.0, 600.0)
        .window_status(native_window_status(&initial_state));
    let fluent_app = match initial_state.theme {
        ThemeMode::Dark => fluent_app.dark_theme(),
        ThemeMode::Light => fluent_app.light_theme(),
    };

    fluent_app.run(move |cx| {
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
            let state_entity = state_entity.clone();
            move |cx| {
                let mut palette = CommandPalette::new(cx).max_results(14);
                palette.set_entries(palette_entries(&state, &state_entity), cx);
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
    let tree = load_account_tree(client).await?;
    let LoadedAccountTree {
        accounts,
        mut folders_by_account,
        labels,
    } = tree;

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

#[derive(Clone, Debug)]
struct LoadedAccountTree {
    accounts: Vec<Account>,
    folders_by_account: BTreeMap<String, Vec<Folder>>,
    labels: Vec<Label>,
}

async fn load_account_tree(client: &EmailClient) -> Result<LoadedAccountTree> {
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

    Ok(LoadedAccountTree {
        accounts,
        folders_by_account,
        labels,
    })
}

fn load_native_preferences() -> NativePreferences {
    let path = native_settings_path();
    match std::fs::read_to_string(&path) {
        Ok(raw) => parse_native_preferences(&raw),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => NativePreferences::default(),
        Err(err) => {
            tracing::warn!("failed to read native FluentGUI settings from {path:?}: {err}");
            NativePreferences::default()
        }
    }
}

fn save_native_preferences(preferences: &NativePreferences) -> Result<()> {
    write_native_preferences(&native_settings_path(), preferences)
}

fn write_native_preferences(path: &Path, preferences: &NativePreferences) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create native settings directory {parent:?}"))?;
    }
    std::fs::write(path, serialize_native_preferences(preferences))
        .with_context(|| format!("write native settings {path:?}"))
}

fn parse_native_preferences(raw: &str) -> NativePreferences {
    let mut preferences = NativePreferences::default();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "theme" => {
                if let Some(theme) = ThemeMode::parse(value) {
                    preferences.theme = theme;
                }
            }
            "density" => {
                if let Ok(level) = value.trim().parse::<u8>() {
                    preferences.density_level = level.min(8);
                }
            }
            _ => {}
        }
    }
    preferences
}

fn serialize_native_preferences(preferences: &NativePreferences) -> String {
    format!(
        "theme={}\ndensity={}\n",
        preferences.theme.as_str(),
        preferences.density_level.min(8)
    )
}

fn native_settings_path() -> PathBuf {
    if let Ok(path) = std::env::var(NATIVE_SETTINGS_ENV) {
        return PathBuf::from(path);
    }

    native_config_dir().join(NATIVE_SETTINGS_FILE)
}

fn native_config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("LOCALAPPDATA")
            .or_else(|_| std::env::var("APPDATA"))
            .unwrap_or_else(|_| std::env::var("TEMP").unwrap_or_else(|_| ".".to_string()));
        return PathBuf::from(base).join("email-rs");
    }

    #[cfg(not(target_os = "windows"))]
    {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("HOME")
                    .map(|home| PathBuf::from(home).join(".config"))
                    .unwrap_or_else(|_| PathBuf::from("/tmp"))
            });
        base.join("email-rs")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct NativePreferences {
    theme: ThemeMode,
    density_level: u8,
}

impl NativePreferences {
    fn from_state(state: &NativeAppState) -> Self {
        Self {
            theme: state.theme.clone(),
            density_level: state.density.level,
        }
    }

    fn apply_to_state(&self, state: &mut NativeAppState) {
        state.theme = self.theme.clone();
        state.density = DensityTokens::new(self.density_level);
    }
}

impl Default for NativePreferences {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Dark,
            density_level: DensityTokens::default().level,
        }
    }
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
struct ComposeState {
    open: bool,
    mode: Option<String>,
    account_id: Option<String>,
    to: Vec<String>,
    cc: Vec<String>,
    bcc: Vec<String>,
    subject: Option<String>,
    body: String,
    in_reply_to: Option<String>,
    quoted_text: Option<String>,
    quoted_from: Option<String>,
    signature: Option<String>,
    signature_loading: bool,
    show_cc: bool,
    show_bcc: bool,
    revision: u64,
    sending: bool,
    error: Option<String>,
    notice: Option<String>,
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
struct ModalStates {
    account_setup_open: bool,
    settings_open: bool,
    advanced_search_open: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SearchState {
    query: String,
    submitted_query: Option<String>,
    condition_group: Option<ConditionGroup>,
    previous_source: Option<NativeMessageSource>,
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
    selected_message_ids: Vec<String>,
    selected_full_message: Option<FullMessage>,
    selected_message_resource: AsyncResource<FullMessage, String>,
    selected_message_labels: Vec<Label>,
    selected_message_labels_resource: AsyncResource<Vec<Label>, String>,
    selected_event: Option<String>,
    search: SearchState,
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
            selected_message_ids: Vec::new(),
            selected_full_message: None,
            selected_message_resource: AsyncResource::new(),
            selected_message_labels: Vec::new(),
            selected_message_labels_resource: AsyncResource::new(),
            selected_event: None,
            search: SearchState::default(),
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
    account_setup_modal: Entity<AccountSetupModal>,
    advanced_search_modal: Entity<AdvancedSearchModal>,
    settings_modal: Entity<SettingsModal>,
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
        let account_setup_modal = cx.new({
            let state = state.clone();
            move |cx| AccountSetupModal::new(cx, state.clone())
        });
        let advanced_search_modal = cx.new({
            let state = state.clone();
            move |cx| AdvancedSearchModal::new(cx, state.clone())
        });
        let settings_modal = cx.new({
            let state = state.clone();
            move |cx| SettingsModal::new(cx, state.clone())
        });
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
            account_setup_modal,
            advanced_search_modal,
            settings_modal,
            sync_updates,
        }
    }

    fn drain_sync_updates(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        let mut refresh_requested = false;
        loop {
            match self.sync_updates.try_recv() {
                Ok(update) => {
                    changed = true;
                    refresh_requested |= sync_update_requests_refresh(&update);
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
        if refresh_requested {
            start_sync_complete_refresh(&self.state, cx);
        }
        if changed {
            cx.notify();
        }
    }

    fn apply_theme_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        let preferences = self.state.update(cx, |state, cx| {
            state.theme = mode.clone();
            let preferences = NativePreferences::from_state(state);
            cx.notify();
            preferences
        });

        match mode {
            ThemeMode::Dark => Theme::apply_dark(cx),
            ThemeMode::Light => Theme::apply_light(cx),
        }

        self.persist_preferences(preferences, cx);
        cx.notify();
    }

    fn adjust_density(&mut self, delta: i8, cx: &mut Context<Self>) {
        let preferences = self.state.update(cx, |state, cx| {
            let level = (state.density.level as i16 + delta as i16).clamp(0, 8) as u8;
            state.density = DensityTokens::new(level);
            let preferences = NativePreferences::from_state(state);
            cx.notify();
            preferences
        });

        self.persist_preferences(preferences, cx);
        cx.notify();
    }

    fn persist_preferences(&mut self, preferences: NativePreferences, cx: &mut Context<Self>) {
        if let Err(err) = save_native_preferences(&preferences) {
            tracing::warn!("failed to save native FluentGUI settings: {err:#}");
            self.state.update(cx, |state, cx| {
                state.last_error = Some(format!("Failed to save native settings: {err}"));
                cx.notify();
            });
        }
    }

    fn refresh_palette_entries(&mut self, cx: &mut Context<Self>) {
        let state = self.state.read(cx).clone();
        let state_entity = self.state.clone();
        self.palette.update(cx, |palette, cx| {
            palette.set_entries(palette_entries(&state, &state_entity), cx)
        });
    }
}

impl Render for EmailDesktopApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("email-desktop-fluent-root")
            .size_full()
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &TogglePalette, _, cx| {
                    app.refresh_palette_entries(cx);
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
                    app.apply_theme_mode(ThemeMode::Light, cx);
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &ApplyDarkTheme, _, cx| {
                    app.apply_theme_mode(ThemeMode::Dark, cx);
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
                    app.adjust_density(1, cx);
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &DecreaseDensity, _, cx| {
                    app.adjust_density(-1, cx);
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &StartCompose, _, cx| {
                    start_new_compose(&app.state, cx);
                    app.refresh_palette_entries(cx);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &StartReplyCompose, _, cx| {
                    start_reply_draft(&app.state, cx);
                    app.refresh_palette_entries(cx);
                    cx.notify();
                }),
            )
            .on_action(cx.listener(
                |app: &mut EmailDesktopApp, _: &StartForwardCompose, _, cx| {
                    start_forward_draft(&app.state, cx);
                    app.refresh_palette_entries(cx);
                    cx.notify();
                },
            ))
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &OpenAdvancedSearch, _, cx| {
                    open_advanced_search_modal(&app.state, &app.advanced_search_modal, cx);
                    app.refresh_palette_entries(cx);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &OpenAccountSetup, _, cx| {
                    open_account_setup_modal(&app.state, &app.account_setup_modal, cx);
                    app.refresh_palette_entries(cx);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &OpenSettings, _, cx| {
                    open_settings_modal(&app.state, &app.settings_modal, cx);
                    app.refresh_palette_entries(cx);
                    cx.notify();
                }),
            )
            .child(self.workspace.clone())
            .child(self.palette.clone())
            .child(self.account_setup_modal.clone())
            .child(self.advanced_search_modal.clone())
            .child(self.settings_modal.clone())
    }
}

#[derive(Clone)]
struct SidebarPlaceholder {
    state: Entity<NativeAppState>,
    searchbox: Entity<Searchbox>,
    context_menu: Option<Entity<ContextMenu>>,
}

impl SidebarPlaceholder {
    fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let searchbox = cx.new({
            let state = state.clone();
            move |_| {
                Searchbox::new()
                    .placeholder("Search mail")
                    .on_change({
                        let state = state.clone();
                        move |query, app_cx| {
                            handle_sidebar_search_change(&state, query.to_string(), app_cx);
                        }
                    })
                    .on_submit({
                        let state = state.clone();
                        move |query, app_cx| {
                            submit_sidebar_search(&state, query.to_string(), app_cx);
                        }
                    })
            }
        });
        Self {
            state,
            searchbox,
            context_menu: None,
        }
    }
}

impl Render for SidebarPlaceholder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let typography = theme.typography;
        let density = state.density.clone();
        sync_searchbox_value(&self.searchbox, &state.search.query, cx);

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
                let folder_menu_id = folder_id.clone();
                let state_entity = self.state.clone();
                let menu_state = self.state.clone();
                let selected = state.selected_folder.as_deref() == Some(folder.id.as_str())
                    && state.selected_label.is_none();
                let folder_context_menu = cx.listener(
                    move |sidebar: &mut SidebarPlaceholder, ev: &MouseDownEvent, _, app_cx| {
                        let open_state = menu_state.clone();
                        let open_folder = folder_menu_id.clone();
                        let mark_state = menu_state.clone();
                        let mark_folder = folder_menu_id.clone();
                        let menu = app_cx.new(|_| {
                            ContextMenu::build(ev.position)
                                .action("Open", move |_, _, app_cx| {
                                    start_message_collection_load(
                                        &open_state,
                                        NativeMessageSource::Folder(open_folder.clone()),
                                        app_cx,
                                    );
                                })
                                .action("Mark all read", move |_, _, app_cx| {
                                    start_mark_folder_read(
                                        &mark_state,
                                        mark_folder.clone(),
                                        app_cx,
                                    );
                                })
                        });
                        sidebar.context_menu = Some(menu);
                        app_cx.notify();
                    },
                );
                account_list = account_list.child(
                    sidebar_nav_row(
                        folder_label(folder),
                        selected,
                        Some(folder.unread_count),
                        folder_indent(folder),
                        folder_icon(folder),
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
                    })
                    .on_mouse_down(MouseButton::Right, folder_context_menu),
                );
            }
        }

        if state.accounts.is_empty() {
            account_list = account_list.child(empty_sidebar_text("No accounts configured", cx));
        }

        let unread = total_unread_count(&state);

        let mut sidebar = div()
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
                            .mb(px(10.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(div().flex_1().min_w_0().child(self.searchbox.clone()))
                            .child(
                                Button::new("sidebar-advanced-search")
                                    .label("Filter")
                                    .appearance(if state.search.condition_group.is_some() {
                                        ButtonAppearance::Neutral
                                    } else {
                                        ButtonAppearance::Subtle
                                    })
                                    .size(ButtonSize::Compact)
                                    .on_click(|_, _, app_cx| {
                                        app_cx.dispatch_action(&OpenAdvancedSearch);
                                    }),
                            ),
                    )
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
                            .id("sidebar-compose")
                            .cursor_pointer()
                            .on_click({
                                let state_entity = self.state.clone();
                                move |_, _, app_cx| {
                                    start_new_compose(&state_entity, app_cx);
                                }
                            })
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
                    .child(
                        Button::new("sidebar-add-account")
                            .label("Add account")
                            .appearance(ButtonAppearance::Subtle)
                            .size(ButtonSize::Compact)
                            .on_click(|_, _, app_cx| {
                                app_cx.dispatch_action(&OpenAccountSetup);
                            }),
                    )
                    .child(section_label("Labels", cx))
                    .child(label_rows(&self.state, &state, cx)),
            )
            .child(sidebar_footer(&self.state, &state, cx));

        if let Some(menu_entity) = &self.context_menu {
            if menu_entity.read(cx).open {
                sidebar = sidebar.child(menu_entity.clone());
            }
        }

        sidebar
    }
}

#[derive(Clone)]
struct MessageListPlaceholder {
    state: Entity<NativeAppState>,
    context_menu: Option<Entity<ContextMenu>>,
}

impl MessageListPlaceholder {
    fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            context_menu: None,
        }
    }
}

impl Render for MessageListPlaceholder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let typography = theme.typography;
        let mailbox_title = current_mailbox_title(&state);
        let message_count = state.messages.len();
        let unread_count = state
            .messages
            .iter()
            .filter(|message| !message.is_read)
            .count();
        let selected_count = state.selected_message_ids.len();
        let mailbox_summary = if state.messages_resource.is_loading() {
            format!("{message_count} cached messages - loading")
        } else if let Some(error) = state.messages_resource.error() {
            format!("{message_count} cached messages - {error}")
        } else if selected_count > 0 {
            format!("{selected_count} selected")
        } else {
            format!("{message_count} messages - {unread_count} unread")
        };
        let can_mark_all_read =
            selected_count == 0 && selected_regular_folder_id(&state).is_some() && unread_count > 0;
        let select_all_state = message_select_all_state(&state);

        let mut list = div()
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
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                Checkbox::new("message-list-select-all")
                                    .state(select_all_state)
                                    .disabled(state.messages.is_empty())
                                    .on_click({
                                        let state_entity = self.state.clone();
                                        move |next, _, _, app_cx| {
                                            app_cx.stop_propagation();
                                            state_entity.update(app_cx, |state, cx| {
                                                set_all_message_selection(
                                                    state,
                                                    matches!(next, CheckboxState::Checked),
                                                );
                                                cx.notify();
                                            });
                                        }
                                    }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(typography.caption.size))
                                    .text_color(if state.messages_resource.is_error() {
                                        colors.status_error
                                    } else {
                                        colors.on_subtle
                                    })
                                    .child(mailbox_summary),
                            )
                            .when(can_mark_all_read, |row| {
                                row.child(
                                    Button::new("message-list-mark-all-read")
                                        .label("Mark all read")
                                        .appearance(ButtonAppearance::Subtle)
                                        .size(ButtonSize::Compact)
                                        .on_click({
                                            let state_entity = self.state.clone();
                                            let folder_id = selected_regular_folder_id(&state)
                                                .unwrap()
                                                .to_string();
                                            move |_, _, app_cx| {
                                                start_mark_folder_read(
                                                    &state_entity,
                                                    folder_id.clone(),
                                                    app_cx,
                                                );
                                            }
                                        }),
                                )
                            })
                            .when(selected_count > 0, |row| {
                                row.child(
                                    Button::new("message-list-bulk-archive")
                                        .label("Archive")
                                        .appearance(ButtonAppearance::Subtle)
                                        .size(ButtonSize::Compact)
                                        .on_click({
                                            let state_entity = self.state.clone();
                                            move |_, _, app_cx| {
                                                start_bulk_message_action(
                                                    &state_entity,
                                                    BulkAction::Archive,
                                                    app_cx,
                                                );
                                            }
                                        }),
                                )
                                .child(
                                    Button::new("message-list-bulk-read")
                                        .label("Mark read")
                                        .appearance(ButtonAppearance::Subtle)
                                        .size(ButtonSize::Compact)
                                        .on_click({
                                            let state_entity = self.state.clone();
                                            move |_, _, app_cx| {
                                                start_bulk_message_action(
                                                    &state_entity,
                                                    BulkAction::MarkRead,
                                                    app_cx,
                                                );
                                            }
                                        }),
                                )
                                .child(
                                    Button::new("message-list-bulk-unread")
                                        .label("Mark unread")
                                        .appearance(ButtonAppearance::Subtle)
                                        .size(ButtonSize::Compact)
                                        .on_click({
                                            let state_entity = self.state.clone();
                                            move |_, _, app_cx| {
                                                start_bulk_message_action(
                                                    &state_entity,
                                                    BulkAction::MarkUnread,
                                                    app_cx,
                                                );
                                            }
                                        }),
                                )
                                .child(
                                    Button::new("message-list-bulk-delete")
                                        .label("Delete")
                                        .appearance(ButtonAppearance::Danger)
                                        .size(ButtonSize::Compact)
                                        .on_click({
                                            let state_entity = self.state.clone();
                                            move |_, _, app_cx| {
                                                start_bulk_message_action(
                                                    &state_entity,
                                                    BulkAction::Delete,
                                                    app_cx,
                                                );
                                            }
                                        }),
                                )
                                .child(
                                    Button::new("message-list-clear-selection")
                                        .label("Clear")
                                        .appearance(ButtonAppearance::Subtle)
                                        .size(ButtonSize::Compact)
                                        .on_click({
                                            let state_entity = self.state.clone();
                                            move |_, _, app_cx| {
                                                state_entity.update(app_cx, |state, cx| {
                                                    state.selected_message_ids.clear();
                                                    cx.notify();
                                                });
                                            }
                                        }),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(mail_message_list(&self.state, &state, cx)),
            );

        if let Some(menu_entity) = &self.context_menu {
            if menu_entity.read(cx).open {
                list = list.child(menu_entity.clone());
            }
        }

        list
    }
}

#[derive(Clone)]
struct ReadingPanePlaceholder {
    state: Entity<NativeAppState>,
    recipient_input: Entity<TokenInput>,
    quick_reply_input: Entity<TextInput>,
    compose_to_input: Entity<TokenInput>,
    compose_cc_input: Entity<TokenInput>,
    compose_bcc_input: Entity<TokenInput>,
    compose_subject_input: Entity<TextInput>,
    compose_body_input: Entity<Textarea>,
    snooze_picker: Entity<DateTimePicker>,
    synced_compose_revision: u64,
}

impl ReadingPanePlaceholder {
    fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let suggestions = recipient_suggestions(state.read(cx));
        let compose_suggestions = suggestions.clone();
        let recipient_input = cx.new(move |_| {
            TokenInput::new()
                .placeholder("Add recipient")
                .suggestions(suggestions.clone())
                .allow_freeform(true)
        });
        let quick_reply_input = cx.new(|_| TextInput::new().placeholder("Write a quick reply"));
        let compose_to_input = cx.new({
            let state = state.clone();
            let suggestions = compose_suggestions.clone();
            move |_| {
                TokenInput::new()
                    .placeholder("recipient@example.com")
                    .suggestions(suggestions.clone())
                    .allow_freeform(true)
                    .on_change({
                        let state = state.clone();
                        move |tokens, app_cx| {
                            update_compose_recipients(
                                &state,
                                ComposeRecipientField::To,
                                tokens,
                                app_cx,
                            );
                        }
                    })
            }
        });
        let compose_cc_input = cx.new({
            let state = state.clone();
            let suggestions = compose_suggestions.clone();
            move |_| {
                TokenInput::new()
                    .placeholder("cc@example.com")
                    .suggestions(suggestions.clone())
                    .allow_freeform(true)
                    .on_change({
                        let state = state.clone();
                        move |tokens, app_cx| {
                            update_compose_recipients(
                                &state,
                                ComposeRecipientField::Cc,
                                tokens,
                                app_cx,
                            );
                        }
                    })
            }
        });
        let compose_bcc_input = cx.new({
            let state = state.clone();
            move |_| {
                TokenInput::new()
                    .placeholder("bcc@example.com")
                    .suggestions(compose_suggestions.clone())
                    .allow_freeform(true)
                    .on_change({
                        let state = state.clone();
                        move |tokens, app_cx| {
                            update_compose_recipients(
                                &state,
                                ComposeRecipientField::Bcc,
                                tokens,
                                app_cx,
                            );
                        }
                    })
            }
        });
        let compose_subject_input = cx.new({
            let state = state.clone();
            move |_| {
                TextInput::new().placeholder("Subject").on_change({
                    let state = state.clone();
                    move |subject, app_cx| {
                        update_compose_subject(&state, subject.to_string(), app_cx);
                    }
                })
            }
        });
        let compose_body_input = cx.new({
            let state = state.clone();
            move |_| {
                Textarea::new()
                    .placeholder("Write your message...")
                    .rows(12)
                    .on_change({
                        let state = state.clone();
                        move |body, app_cx| {
                            update_compose_body(&state, body.to_string(), app_cx);
                        }
                    })
            }
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
            quick_reply_input,
            compose_to_input,
            compose_cc_input,
            compose_bcc_input,
            compose_subject_input,
            compose_body_input,
            snooze_picker,
            synced_compose_revision: 0,
        }
    }
}

impl Render for ReadingPanePlaceholder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        self.sync_compose_inputs(&state, cx);
        if state.compose.open {
            return compose_placeholder(
                &self.state,
                &state,
                self.compose_to_input.clone(),
                self.compose_cc_input.clone(),
                self.compose_bcc_input.clone(),
                self.compose_subject_input.clone(),
                self.compose_body_input.clone(),
                cx,
            );
        }

        reading_placeholder(
            &self.state,
            &state,
            self.recipient_input.clone(),
            self.quick_reply_input.clone(),
            self.snooze_picker.clone(),
            cx,
        )
    }
}

impl ReadingPanePlaceholder {
    fn sync_compose_inputs(&mut self, state: &NativeAppState, cx: &mut Context<Self>) {
        if state.compose.revision == self.synced_compose_revision {
            return;
        }

        self.compose_to_input.update(cx, |input, cx| {
            input.set_tokens(compose_tokens(&state.compose.to), cx);
        });
        self.compose_cc_input.update(cx, |input, cx| {
            input.set_tokens(compose_tokens(&state.compose.cc), cx);
        });
        self.compose_bcc_input.update(cx, |input, cx| {
            input.set_tokens(compose_tokens(&state.compose.bcc), cx);
        });
        self.compose_subject_input.update(cx, |input, cx| {
            input.set_value(state.compose.subject.clone().unwrap_or_default(), cx);
        });
        self.compose_body_input.update(cx, |input, cx| {
            input.set_value(state.compose.body.clone(), cx);
        });
        self.synced_compose_revision = state.compose.revision;
    }
}

#[derive(Clone)]
struct AdvancedConditionRow {
    key: u64,
    condition: Condition,
    value_input: Entity<TextInput>,
    field_dropdown: Entity<Dropdown>,
    operator_dropdown: Entity<Dropdown>,
    bool_dropdown: Entity<Dropdown>,
}

#[derive(Clone)]
struct AdvancedSearchModal {
    state: Entity<NativeAppState>,
    match_mode: MatchMode,
    rows: Vec<AdvancedConditionRow>,
    next_condition_id: u64,
}

impl AdvancedSearchModal {
    fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let mut modal = Self {
            state,
            match_mode: MatchMode::All,
            rows: Vec::new(),
            next_condition_id: 1,
        };
        modal.reset_to_group(None, cx);
        modal
    }

    fn reset_to_group(&mut self, group: Option<ConditionGroup>, cx: &mut Context<Self>) {
        self.rows.clear();
        self.next_condition_id = 1;
        let group = group.unwrap_or_else(default_condition_group);
        self.match_mode = group.match_mode;
        for condition in group.conditions {
            self.add_condition(condition, cx);
        }
        if self.rows.is_empty() {
            let id = self.take_condition_id();
            self.add_condition(new_advanced_condition(id), cx);
        }
        cx.notify();
    }

    fn take_condition_id(&mut self) -> String {
        let id = format!("native-condition-{}", self.next_condition_id);
        self.next_condition_id = self.next_condition_id.saturating_add(1);
        id
    }

    fn add_condition(&mut self, condition: Condition, cx: &mut Context<Self>) {
        let key = self.next_condition_id;
        self.next_condition_id = self.next_condition_id.saturating_add(1);
        let condition_id = condition.id.clone();
        let initial_value = condition.value.clone();
        let modal_entity = cx.entity();
        let value_input = cx.new(move |_| {
            TextInput::new()
                .value(initial_value.clone())
                .placeholder("Value")
                .on_change({
                    let modal_entity = modal_entity.clone();
                    let condition_id = condition_id.clone();
                    move |value, app_cx| {
                        let _ = modal_entity.update(app_cx, |modal, cx| {
                            modal.set_condition_value(&condition_id, value.to_string());
                            cx.notify();
                        });
                    }
                })
        });
        let field_condition_id = condition.id.clone();
        let field_modal = cx.entity();
        let field_selected = condition_field_value(&condition.field);
        let field_dropdown = cx.new(move |_| {
            condition_field_dropdown(
                ("advanced-field", key),
                field_selected,
                field_modal.clone(),
                field_condition_id.clone(),
            )
        });
        let operator_condition_id = condition.id.clone();
        let operator_modal = cx.entity();
        let operator_selected = condition_operator_value(&condition.operator);
        let operator_dropdown = cx.new(move |_| {
            condition_operator_dropdown(
                ("advanced-op", key),
                operator_selected,
                operator_modal.clone(),
                operator_condition_id.clone(),
            )
        });
        let bool_condition_id = condition.id.clone();
        let bool_modal = cx.entity();
        let bool_selected = if condition.value.trim() == "false" {
            "false"
        } else {
            "true"
        };
        let bool_dropdown = cx.new(move |_| {
            boolean_dropdown(
                ("advanced-bool", key),
                bool_selected,
                bool_modal.clone(),
                bool_condition_id.clone(),
            )
        });
        self.rows.push(AdvancedConditionRow {
            key,
            condition,
            value_input,
            field_dropdown,
            operator_dropdown,
            bool_dropdown,
        });
    }

    fn add_blank_condition(&mut self, cx: &mut Context<Self>) {
        let id = self.take_condition_id();
        self.add_condition(new_advanced_condition(id), cx);
        cx.notify();
    }

    fn remove_condition(&mut self, condition_id: &str, cx: &mut Context<Self>) {
        self.rows
            .retain(|row| row.condition.id.as_str() != condition_id);
        if self.rows.is_empty() {
            let id = self.take_condition_id();
            self.add_condition(new_advanced_condition(id), cx);
        }
        cx.notify();
    }

    fn set_condition_field(
        &mut self,
        condition_id: &str,
        field: ConditionField,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.condition.id == condition_id)
        {
            row.condition.field = field.clone();
            row.condition.operator = ConditionOperator::Contains;
            row.condition.value = default_condition_value(&field).to_string();
            let value = row.condition.value.clone();
            row.value_input.update(cx, |input, cx| {
                input.set_value(value, cx);
            });
            row.operator_dropdown.update(cx, |dropdown, cx| {
                dropdown.set_selected(condition_operator_value(&ConditionOperator::Contains), cx);
            });
            row.bool_dropdown.update(cx, |dropdown, cx| {
                dropdown.set_selected(default_condition_value(&field), cx);
            });
            cx.notify();
        }
    }

    fn set_condition_operator(&mut self, condition_id: &str, operator: ConditionOperator) {
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.condition.id == condition_id)
        {
            row.condition.operator = operator;
        }
    }

    fn set_condition_value(&mut self, condition_id: &str, value: String) {
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.condition.id == condition_id)
        {
            row.condition.value = value;
        }
    }

    fn set_match_mode(&mut self, match_mode: MatchMode) {
        self.match_mode = match_mode;
    }

    fn condition_group(&self) -> ConditionGroup {
        ConditionGroup {
            match_mode: self.match_mode.clone(),
            conditions: self.rows.iter().map(|row| row.condition.clone()).collect(),
        }
    }

    fn has_active_condition(&self) -> bool {
        condition_group_has_active(&self.condition_group())
    }
}

impl Render for AdvancedSearchModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        if !state.modals.advanced_search_open {
            return div();
        }

        let theme = cx.theme();
        let colors = theme.colors.clone();
        let typography = theme.typography;
        let mut backdrop = colors.surface_dim;
        backdrop.a = 0.58;
        let has_active = self.has_active_condition();
        let modal_entity = cx.entity();

        let mut conditions = div().flex().flex_col().gap(px(8.0));
        for row in self.rows.clone() {
            conditions = conditions.child(advanced_condition_row(row, modal_entity.clone(), cx));
        }

        div()
            .absolute()
            .inset_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(backdrop)
            .on_mouse_down(MouseButton::Left, {
                let state_entity = self.state.clone();
                move |_, _, app_cx| {
                    close_advanced_search_modal(&state_entity, app_cx);
                }
            })
            .child(
                div()
                    .w(px(760.0))
                    .max_h(px(560.0))
                    .flex()
                    .flex_col()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(colors.stroke_neutral)
                    .bg(colors.surface)
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, app_cx| {
                        app_cx.stop_propagation();
                    })
                    .child(
                        div()
                            .px(px(18.0))
                            .py(px(14.0))
                            .border_b_1()
                            .border_color(colors.stroke_neutral_subtle)
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(typography.subtitle.size))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.on_neutral)
                                    .child("Advanced search"),
                            )
                            .child(
                                Button::new("advanced-search-close")
                                    .label("Close")
                                    .appearance(ButtonAppearance::Subtle)
                                    .size(ButtonSize::Compact)
                                    .on_click({
                                        let state_entity = self.state.clone();
                                        move |_, _, app_cx| {
                                            close_advanced_search_modal(&state_entity, app_cx);
                                        }
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .id("advanced-search-scroll")
                            .overflow_y_scroll()
                            .px(px(18.0))
                            .py(px(16.0))
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(advanced_label("Match", cx))
                                    .child(
                                        Button::new("advanced-match-all")
                                            .label("All")
                                            .appearance(if self.match_mode == MatchMode::All {
                                                ButtonAppearance::Neutral
                                            } else {
                                                ButtonAppearance::Subtle
                                            })
                                            .size(ButtonSize::Compact)
                                            .on_click({
                                                let modal_entity = modal_entity.clone();
                                                move |_, _, app_cx| {
                                                    let _ =
                                                        modal_entity.update(app_cx, |modal, cx| {
                                                            modal.set_match_mode(MatchMode::All);
                                                            cx.notify();
                                                        });
                                                }
                                            }),
                                    )
                                    .child(
                                        Button::new("advanced-match-any")
                                            .label("Any")
                                            .appearance(if self.match_mode == MatchMode::Any {
                                                ButtonAppearance::Neutral
                                            } else {
                                                ButtonAppearance::Subtle
                                            })
                                            .size(ButtonSize::Compact)
                                            .on_click({
                                                let modal_entity = modal_entity.clone();
                                                move |_, _, app_cx| {
                                                    let _ =
                                                        modal_entity.update(app_cx, |modal, cx| {
                                                            modal.set_match_mode(MatchMode::Any);
                                                            cx.notify();
                                                        });
                                                }
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(typography.body.size))
                                            .text_color(colors.on_subtle)
                                            .child("of the following conditions"),
                                    ),
                            )
                            .child(conditions)
                            .child(
                                Button::new("advanced-add-condition")
                                    .label("Add condition")
                                    .appearance(ButtonAppearance::Subtle)
                                    .size(ButtonSize::Compact)
                                    .on_click({
                                        let modal_entity = modal_entity.clone();
                                        move |_, _, app_cx| {
                                            let _ = modal_entity.update(app_cx, |modal, cx| {
                                                modal.add_blank_condition(cx);
                                            });
                                        }
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .border_t_1()
                            .border_color(colors.stroke_neutral_subtle)
                            .px(px(18.0))
                            .py(px(12.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                Button::new("advanced-clear")
                                    .label("Clear")
                                    .appearance(ButtonAppearance::Subtle)
                                    .size(ButtonSize::Compact)
                                    .on_click({
                                        let modal_entity = modal_entity.clone();
                                        move |_, _, app_cx| {
                                            let _ = modal_entity.update(app_cx, |modal, cx| {
                                                modal.reset_to_group(None, cx);
                                            });
                                        }
                                    }),
                            )
                            .child(div().flex_1())
                            .child(
                                Button::new("advanced-cancel")
                                    .label("Cancel")
                                    .appearance(ButtonAppearance::Subtle)
                                    .size(ButtonSize::Compact)
                                    .on_click({
                                        let state_entity = self.state.clone();
                                        move |_, _, app_cx| {
                                            close_advanced_search_modal(&state_entity, app_cx);
                                        }
                                    }),
                            )
                            .child(
                                Button::new("advanced-submit")
                                    .label("Search")
                                    .appearance(ButtonAppearance::Accent)
                                    .size(ButtonSize::Compact)
                                    .disabled(!has_active)
                                    .on_click({
                                        let state_entity = self.state.clone();
                                        let modal_entity = modal_entity.clone();
                                        move |_, _, app_cx| {
                                            let group = modal_entity
                                                .update(app_cx, |modal, _| modal.condition_group());
                                            start_advanced_search(&state_entity, group, app_cx);
                                        }
                                    }),
                            ),
                    ),
            )
    }
}

fn advanced_condition_row(
    row: AdvancedConditionRow,
    modal_entity: Entity<AdvancedSearchModal>,
    cx: &mut Context<AdvancedSearchModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let field_type = condition_field_type(&row.condition.field);
    let condition_id = row.condition.id.clone();
    let mut line = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(div().w(px(160.0)).child(row.field_dropdown.clone()));

    match field_type {
        ConditionFieldType::Text => {
            line = line
                .child(div().w(px(156.0)).child(row.operator_dropdown.clone()))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(180.0))
                        .child(row.value_input.clone()),
                );
        }
        ConditionFieldType::Boolean => {
            line = line
                .child(div().w(px(156.0)).child(row.bool_dropdown.clone()))
                .child(div().flex_1());
        }
        ConditionFieldType::Date => {
            line = line
                .child(
                    div()
                        .w(px(156.0))
                        .text_size(px(theme.typography.caption.size))
                        .text_color(colors.on_subtle)
                        .child("YYYY-MM-DD"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(180.0))
                        .child(row.value_input.clone()),
                );
        }
    }

    line.child(
        Button::new(("advanced-remove", row.key))
            .label("Remove")
            .appearance(ButtonAppearance::Subtle)
            .size(ButtonSize::Compact)
            .on_click({
                let modal_entity = modal_entity.clone();
                move |_, _, app_cx| {
                    let _ = modal_entity.update(app_cx, |modal, cx| {
                        modal.remove_condition(&condition_id, cx);
                    });
                }
            }),
    )
}

fn condition_field_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: &'static str,
    modal_entity: Entity<AdvancedSearchModal>,
    condition_id: String,
) -> Dropdown {
    Dropdown::new(id)
        .selected(selected)
        .option("from", "From")
        .option("to", "To")
        .option("cc", "Cc")
        .option("subject", "Subject")
        .option("body", "Body")
        .option("filename", "Filename")
        .option("has_attachment", "Has attachment")
        .option("is_read", "Is read")
        .option("is_flagged", "Is starred")
        .option("date_after", "Date after")
        .option("date_before", "Date before")
        .on_select(move |value, app_cx| {
            let Some(field) = parse_condition_field(&value.to_string()) else {
                return;
            };
            let _ = modal_entity.update(app_cx, |modal, cx| {
                modal.set_condition_field(&condition_id, field, cx);
            });
        })
}

fn condition_operator_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: &'static str,
    modal_entity: Entity<AdvancedSearchModal>,
    condition_id: String,
) -> Dropdown {
    Dropdown::new(id)
        .selected(selected)
        .option("contains", "contains")
        .option("not_contains", "doesn't contain")
        .option("equals", "equals")
        .option("starts_with", "starts with")
        .on_select(move |value, app_cx| {
            let Some(operator) = parse_condition_operator(&value.to_string()) else {
                return;
            };
            let _ = modal_entity.update(app_cx, |modal, cx| {
                modal.set_condition_operator(&condition_id, operator);
                cx.notify();
            });
        })
}

fn boolean_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: &'static str,
    modal_entity: Entity<AdvancedSearchModal>,
    condition_id: String,
) -> Dropdown {
    Dropdown::new(id)
        .selected(selected)
        .option("true", "Yes")
        .option("false", "No")
        .on_select(move |value, app_cx| {
            let _ = modal_entity.update(app_cx, |modal, cx| {
                modal.set_condition_value(&condition_id, value.to_string());
                cx.notify();
            });
        })
}

fn rule_condition_field_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: &'static str,
    modal_entity: Entity<SettingsModal>,
    condition_id: String,
) -> Dropdown {
    Dropdown::new(id)
        .selected(selected)
        .option("from", "From")
        .option("to", "To")
        .option("cc", "Cc")
        .option("subject", "Subject")
        .option("body", "Body")
        .option("filename", "Filename")
        .option("has_attachment", "Has attachment")
        .option("is_read", "Is read")
        .option("is_flagged", "Is starred")
        .option("date_after", "Date after")
        .option("date_before", "Date before")
        .on_select(move |value, app_cx| {
            let Some(field) = parse_condition_field(&value.to_string()) else {
                return;
            };
            let _ = modal_entity.update(app_cx, |modal, cx| {
                modal.set_rule_condition_field(&condition_id, field, cx);
            });
        })
}

fn rule_condition_operator_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: &'static str,
    modal_entity: Entity<SettingsModal>,
    condition_id: String,
) -> Dropdown {
    Dropdown::new(id)
        .selected(selected)
        .option("contains", "contains")
        .option("not_contains", "doesn't contain")
        .option("equals", "equals")
        .option("starts_with", "starts with")
        .on_select(move |value, app_cx| {
            let Some(operator) = parse_condition_operator(&value.to_string()) else {
                return;
            };
            let _ = modal_entity.update(app_cx, |modal, cx| {
                modal.set_rule_condition_operator(&condition_id, operator);
                cx.notify();
            });
        })
}

fn rule_boolean_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: &'static str,
    modal_entity: Entity<SettingsModal>,
    condition_id: String,
) -> Dropdown {
    Dropdown::new(id)
        .selected(selected)
        .option("true", "Yes")
        .option("false", "No")
        .on_select(move |value, app_cx| {
            let _ = modal_entity.update(app_cx, |modal, cx| {
                modal.set_rule_condition_value(&condition_id, value.to_string());
                cx.notify();
            });
        })
}

fn rule_action_type_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: &str,
    modal_entity: Entity<SettingsModal>,
    action_key: u64,
) -> Dropdown {
    Dropdown::new(id)
        .selected(selected.to_string())
        .option("mark_read", "Mark as read")
        .option("mark_unread", "Mark as unread")
        .option("flag", "Flag message")
        .option("unflag", "Unflag message")
        .option("archive", "Archive")
        .option("delete", "Delete")
        .option("move_to_folder", "Move to folder")
        .on_select(move |value, app_cx| {
            let action_type = value.to_string();
            if !is_supported_rule_action(&action_type) {
                return;
            }
            let _ = modal_entity.update(app_cx, |modal, cx| {
                modal.set_rule_action_type(action_key, action_type);
                cx.notify();
            });
        })
}

fn rule_action_folder_dropdown(
    id: impl Into<gpui::ElementId>,
    selected: Option<String>,
    folders: Vec<Folder>,
    modal_entity: Entity<SettingsModal>,
    action_key: u64,
) -> Dropdown {
    let mut dropdown = Dropdown::new(id).placeholder("Select folder");
    if let Some(selected) = selected.filter(|value| !value.trim().is_empty()) {
        dropdown = dropdown.selected(selected);
    }
    for folder in folders {
        dropdown = dropdown.option(folder.id.clone(), rule_folder_option_label(&folder));
    }
    dropdown.on_select(move |value, app_cx| {
        let selected = value.to_string();
        let _ = modal_entity.update(app_cx, |modal, cx| {
            modal.set_rule_action_value(action_key, Some(selected));
            cx.notify();
        });
    })
}

fn advanced_label(label: &'static str, cx: &mut Context<AdvancedSearchModal>) -> gpui::Div {
    let theme = cx.theme();
    div()
        .text_size(px(theme.typography.caption.size))
        .text_color(theme.colors.on_subtle)
        .child(label)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConditionFieldType {
    Text,
    Boolean,
    Date,
}

fn condition_field_type(field: &ConditionField) -> ConditionFieldType {
    match field {
        ConditionField::HasAttachment | ConditionField::IsRead | ConditionField::IsFlagged => {
            ConditionFieldType::Boolean
        }
        ConditionField::DateAfter | ConditionField::DateBefore => ConditionFieldType::Date,
        _ => ConditionFieldType::Text,
    }
}

fn default_condition_group() -> ConditionGroup {
    ConditionGroup {
        match_mode: MatchMode::All,
        conditions: vec![new_advanced_condition("native-condition-1".to_string())],
    }
}

fn new_advanced_condition(id: String) -> Condition {
    Condition {
        id,
        field: ConditionField::From,
        operator: ConditionOperator::Contains,
        value: String::new(),
    }
}

fn default_condition_value(field: &ConditionField) -> &'static str {
    if condition_field_type(field) == ConditionFieldType::Boolean {
        "true"
    } else {
        ""
    }
}

fn condition_group_has_active(group: &ConditionGroup) -> bool {
    group
        .conditions
        .iter()
        .any(|condition| !condition.value.trim().is_empty())
}

fn condition_field_value(field: &ConditionField) -> &'static str {
    match field {
        ConditionField::From => "from",
        ConditionField::To => "to",
        ConditionField::Cc => "cc",
        ConditionField::Subject => "subject",
        ConditionField::Body => "body",
        ConditionField::Filename => "filename",
        ConditionField::HasAttachment => "has_attachment",
        ConditionField::IsRead => "is_read",
        ConditionField::IsFlagged => "is_flagged",
        ConditionField::DateAfter => "date_after",
        ConditionField::DateBefore => "date_before",
    }
}

fn parse_condition_field(value: &str) -> Option<ConditionField> {
    match value {
        "from" => Some(ConditionField::From),
        "to" => Some(ConditionField::To),
        "cc" => Some(ConditionField::Cc),
        "subject" => Some(ConditionField::Subject),
        "body" => Some(ConditionField::Body),
        "filename" => Some(ConditionField::Filename),
        "has_attachment" => Some(ConditionField::HasAttachment),
        "is_read" => Some(ConditionField::IsRead),
        "is_flagged" => Some(ConditionField::IsFlagged),
        "date_after" => Some(ConditionField::DateAfter),
        "date_before" => Some(ConditionField::DateBefore),
        _ => None,
    }
}

fn condition_operator_value(operator: &ConditionOperator) -> &'static str {
    match operator {
        ConditionOperator::Contains => "contains",
        ConditionOperator::NotContains => "not_contains",
        ConditionOperator::Equals => "equals",
        ConditionOperator::StartsWith => "starts_with",
    }
}

fn parse_condition_operator(value: &str) -> Option<ConditionOperator> {
    match value {
        "contains" => Some(ConditionOperator::Contains),
        "not_contains" => Some(ConditionOperator::NotContains),
        "equals" => Some(ConditionOperator::Equals),
        "starts_with" => Some(ConditionOperator::StartsWith),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountSetupMode {
    Choice,
    ManualImap,
    MicrosoftAppPassword,
}

impl AccountSetupMode {
    fn title(self) -> &'static str {
        match self {
            Self::Choice => "Connect account",
            Self::ManualImap => "Manual IMAP/SMTP",
            Self::MicrosoftAppPassword => "Microsoft app password",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountSetupProvider {
    Gmail,
    Microsoft,
}

impl AccountSetupProvider {
    fn label(self) -> &'static str {
        match self {
            Self::Gmail => "Gmail",
            Self::Microsoft => "Microsoft 365",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AccountSetupFormDraft {
    name: String,
    email: String,
    imap_host: String,
    imap_port: String,
    imap_password: String,
    smtp_host: String,
    smtp_port: String,
    smtp_password: String,
    same_password: bool,
}

struct AccountSetupModal {
    state: Entity<NativeAppState>,
    focus_handle: FocusHandle,
    focused_on_open: bool,
    mode: AccountSetupMode,
    name_input: Entity<TextInput>,
    email_input: Entity<TextInput>,
    imap_host_input: Entity<TextInput>,
    imap_port_input: Entity<TextInput>,
    imap_password_input: Entity<TextInput>,
    smtp_host_input: Entity<TextInput>,
    smtp_port_input: Entity<TextInput>,
    smtp_password_input: Entity<TextInput>,
    same_password: bool,
    submitting: bool,
    refreshing: bool,
    oauth_loading: Option<AccountSetupProvider>,
    status: Option<(String, bool)>,
}

impl AccountSetupModal {
    fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            focus_handle: cx.focus_handle().tab_stop(true),
            focused_on_open: false,
            mode: AccountSetupMode::Choice,
            name_input: cx.new(|_| TextInput::new().placeholder("Display name")),
            email_input: cx.new(|_| TextInput::new().placeholder("name@example.com")),
            imap_host_input: cx.new(|_| TextInput::new().placeholder("imap.example.com")),
            imap_port_input: cx.new(|_| TextInput::new().placeholder("993")),
            imap_password_input: cx
                .new(|_| TextInput::new().placeholder("IMAP password").masked(true)),
            smtp_host_input: cx.new(|_| TextInput::new().placeholder("smtp.example.com")),
            smtp_port_input: cx.new(|_| TextInput::new().placeholder("587")),
            smtp_password_input: cx
                .new(|_| TextInput::new().placeholder("SMTP password").masked(true)),
            same_password: true,
            submitting: false,
            refreshing: false,
            oauth_loading: None,
            status: None,
        }
    }

    fn is_busy(&self) -> bool {
        self.submitting || self.refreshing || self.oauth_loading.is_some()
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        self.mode = AccountSetupMode::Choice;
        self.same_password = true;
        self.submitting = false;
        self.refreshing = false;
        self.oauth_loading = None;
        self.status = None;
        self.clear_inputs(cx);
        cx.notify();
    }

    fn clear_inputs(&mut self, cx: &mut Context<Self>) {
        self.name_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.email_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.imap_host_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.imap_port_input
            .update(cx, |input, cx| input.set_value("993", cx));
        self.imap_password_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.smtp_host_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.smtp_port_input
            .update(cx, |input, cx| input.set_value("587", cx));
        self.smtp_password_input
            .update(cx, |input, cx| input.set_value("", cx));
    }

    fn show_mode(&mut self, mode: AccountSetupMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.same_password = true;
        self.submitting = false;
        self.refreshing = false;
        self.oauth_loading = None;
        self.status = None;
        self.clear_inputs(cx);
        if mode == AccountSetupMode::MicrosoftAppPassword {
            self.imap_host_input.update(cx, |input, cx| {
                input.set_value("outlook.office365.com", cx);
            });
            self.smtp_host_input.update(cx, |input, cx| {
                input.set_value("smtp.office365.com", cx);
            });
        }
        cx.notify();
    }

    fn set_status(&mut self, status: Option<(String, bool)>, cx: &mut Context<Self>) {
        self.status = status;
        self.submitting = false;
        self.refreshing = false;
        self.oauth_loading = None;
        cx.notify();
    }
}

impl Render for AccountSetupModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        let first_run = state.accounts.is_empty();
        if !first_run && !state.modals.account_setup_open {
            self.focused_on_open = false;
            return div();
        }
        if !self.focused_on_open {
            window.focus(&self.focus_handle);
            self.focused_on_open = true;
        }

        let theme = cx.theme();
        let colors = theme.colors.clone();
        let typography = theme.typography;
        let mut backdrop = colors.surface_dim;
        backdrop.a = 0.6;
        let modal_entity = cx.entity();
        let can_close = !first_run && !self.is_busy();

        div()
            .absolute()
            .inset_0()
            .size_full()
            .tab_index(0)
            .key_context("AccountSetupModal")
            .track_focus(&self.focus_handle)
            .flex()
            .items_center()
            .justify_center()
            .bg(backdrop)
            .on_key_down({
                let state_entity = self.state.clone();
                let modal_entity = modal_entity.clone();
                move |ev: &KeyDownEvent, _, app_cx| {
                    if handle_account_setup_key(
                        &state_entity,
                        &modal_entity,
                        ev.keystroke.key.as_str(),
                        app_cx,
                    ) {
                        app_cx.stop_propagation();
                    }
                }
            })
            .on_mouse_down(MouseButton::Left, {
                let state_entity = self.state.clone();
                move |_, _, app_cx| {
                    if can_close {
                        close_account_setup_modal(&state_entity, app_cx);
                    }
                }
            })
            .child(
                div()
                    .w(px(620.0))
                    .max_h(px(680.0))
                    .flex()
                    .flex_col()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(colors.stroke_neutral)
                    .bg(colors.surface)
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, app_cx| {
                        app_cx.stop_propagation();
                    })
                    .child(
                        div()
                            .px(px(18.0))
                            .py(px(14.0))
                            .border_b_1()
                            .border_color(colors.stroke_neutral_subtle)
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_size(px(typography.subtitle.size))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(colors.on_neutral)
                                            .child(if first_run {
                                                "Set up email-rs"
                                            } else {
                                                "Add account"
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(typography.caption.size))
                                            .text_color(colors.on_subtle)
                                            .child(self.mode.title()),
                                    ),
                            )
                            .when(can_close, |header| {
                                header.child(
                                    Button::new("account-setup-close")
                                        .label("Close")
                                        .appearance(ButtonAppearance::Subtle)
                                        .size(ButtonSize::Compact)
                                        .on_click({
                                            let state_entity = self.state.clone();
                                            move |_, _, app_cx| {
                                                close_account_setup_modal(&state_entity, app_cx);
                                            }
                                        }),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id("account-setup-body")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p(px(18.0))
                            .child(account_setup_body(
                                &self.state,
                                &modal_entity,
                                first_run,
                                self,
                                cx,
                            )),
                    ),
            )
    }
}

fn account_setup_body(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    first_run: bool,
    modal: &AccountSetupModal,
    cx: &mut Context<AccountSetupModal>,
) -> gpui::AnyElement {
    match modal.mode {
        AccountSetupMode::Choice => {
            account_setup_choice_body(state_entity, modal_entity, modal, cx).into_any_element()
        }
        AccountSetupMode::ManualImap | AccountSetupMode::MicrosoftAppPassword => {
            account_setup_form_body(state_entity, modal_entity, first_run, modal, cx)
                .into_any_element()
        }
    }
}

fn account_setup_choice_body(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    modal: &AccountSetupModal,
    cx: &mut Context<AccountSetupModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let busy = modal.is_busy();
    let gmail_loading = modal.oauth_loading == Some(AccountSetupProvider::Gmail);
    let microsoft_loading = modal.oauth_loading == Some(AccountSetupProvider::Microsoft);

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            Button::new("account-setup-gmail")
                .label(if gmail_loading {
                    "Opening Gmail..."
                } else {
                    "Connect Gmail"
                })
                .appearance(ButtonAppearance::Accent)
                .disabled(busy)
                .on_click({
                    let state_entity = state_entity.clone();
                    let modal_entity = modal_entity.clone();
                    move |_, _, app_cx| {
                        start_account_setup_oauth(
                            &state_entity,
                            &modal_entity,
                            AccountSetupProvider::Gmail,
                            app_cx,
                        );
                    }
                }),
        )
        .child(
            Button::new("account-setup-microsoft-oauth")
                .label(if microsoft_loading {
                    "Opening Microsoft 365..."
                } else {
                    "Connect Microsoft 365"
                })
                .appearance(ButtonAppearance::Neutral)
                .disabled(busy)
                .on_click({
                    let state_entity = state_entity.clone();
                    let modal_entity = modal_entity.clone();
                    move |_, _, app_cx| {
                        start_account_setup_oauth(
                            &state_entity,
                            &modal_entity,
                            AccountSetupProvider::Microsoft,
                            app_cx,
                        );
                    }
                }),
        )
        .child(
            div()
                .h(px(1.0))
                .my(px(4.0))
                .bg(colors.stroke_neutral_subtle),
        )
        .child(
            Button::new("account-setup-microsoft-app-password")
                .label("Use Microsoft app password")
                .appearance(ButtonAppearance::Subtle)
                .disabled(busy)
                .on_click({
                    let modal_entity = modal_entity.clone();
                    move |_, _, app_cx| {
                        let _ = modal_entity.update(app_cx, |modal, cx| {
                            modal.show_mode(AccountSetupMode::MicrosoftAppPassword, cx);
                        });
                    }
                }),
        )
        .child(
            Button::new("account-setup-manual")
                .label("Manual IMAP/SMTP")
                .appearance(ButtonAppearance::Subtle)
                .disabled(busy)
                .on_click({
                    let modal_entity = modal_entity.clone();
                    move |_, _, app_cx| {
                        let _ = modal_entity.update(app_cx, |modal, cx| {
                            modal.show_mode(AccountSetupMode::ManualImap, cx);
                        });
                    }
                }),
        )
        .when_some(modal.status.as_ref(), |body, status| {
            body.child(account_setup_status(status, cx)).child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Button::new("account-setup-refresh")
                            .label(if modal.refreshing {
                                "Refreshing..."
                            } else {
                                "Refresh accounts"
                            })
                            .appearance(ButtonAppearance::Neutral)
                            .size(ButtonSize::Compact)
                            .disabled(busy)
                            .on_click({
                                let state_entity = state_entity.clone();
                                let modal_entity = modal_entity.clone();
                                move |_, _, app_cx| {
                                    start_account_setup_refresh(
                                        &state_entity,
                                        &modal_entity,
                                        app_cx,
                                    );
                                }
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(typography.caption.size))
                            .text_color(colors.on_subtle)
                            .child("Use after browser authorization completes."),
                    ),
            )
        })
}

fn account_setup_form_body(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    first_run: bool,
    modal: &AccountSetupModal,
    cx: &mut Context<AccountSetupModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let busy = modal.is_busy();
    let is_manual = modal.mode == AccountSetupMode::ManualImap;

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(account_setup_field("Name", modal.name_input.clone(), cx))
        .child(account_setup_field("Email", modal.email_input.clone(), cx))
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .child(div().flex_1().min_w_0().child(account_setup_field(
                    "IMAP host",
                    modal.imap_host_input.clone(),
                    cx,
                )))
                .child(div().w(px(120.0)).child(account_setup_field(
                    "Port",
                    modal.imap_port_input.clone(),
                    cx,
                ))),
        )
        .child(account_setup_field(
            if is_manual {
                "IMAP password"
            } else {
                "App password"
            },
            modal.imap_password_input.clone(),
            cx,
        ))
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .child(div().flex_1().min_w_0().child(account_setup_field(
                    "SMTP host",
                    modal.smtp_host_input.clone(),
                    cx,
                )))
                .child(div().w(px(120.0)).child(account_setup_field(
                    "Port",
                    modal.smtp_port_input.clone(),
                    cx,
                ))),
        )
        .when(is_manual, |form| {
            form.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Checkbox::new("account-setup-same-password")
                            .state(if modal.same_password {
                                CheckboxState::Checked
                            } else {
                                CheckboxState::Unchecked
                            })
                            .disabled(busy)
                            .on_click({
                                let modal_entity = modal_entity.clone();
                                move |new_state, _, _, app_cx| {
                                    let same_password = matches!(new_state, CheckboxState::Checked);
                                    let _ = modal_entity.update(app_cx, |modal, cx| {
                                        modal.same_password = same_password;
                                        modal.status = None;
                                        cx.notify();
                                    });
                                }
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(typography.body.size))
                            .text_color(colors.on_neutral)
                            .child("Use IMAP password for SMTP"),
                    ),
            )
        })
        .when(is_manual && !modal.same_password, |form| {
            form.child(account_setup_field(
                "SMTP password",
                modal.smtp_password_input.clone(),
                cx,
            ))
        })
        .when_some(modal.status.as_ref(), |form, status| {
            form.child(account_setup_status(status, cx))
        })
        .child(
            div()
                .pt(px(4.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.0))
                .child(
                    Button::new("account-setup-back")
                        .label(if first_run { "Back" } else { "Cancel" })
                        .appearance(ButtonAppearance::Subtle)
                        .disabled(busy)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                cancel_account_setup_form(
                                    &state_entity,
                                    &modal_entity,
                                    first_run,
                                    app_cx,
                                );
                            }
                        }),
                )
                .child(
                    Button::new("account-setup-submit")
                        .label(if modal.submitting {
                            "Adding..."
                        } else {
                            "Add account"
                        })
                        .appearance(ButtonAppearance::Accent)
                        .disabled(busy)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                start_account_setup_create(&state_entity, &modal_entity, app_cx);
                            }
                        }),
                ),
        )
}

fn account_setup_field(
    label: &'static str,
    input: Entity<TextInput>,
    cx: &mut Context<AccountSetupModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    div()
        .flex()
        .flex_col()
        .gap(px(5.0))
        .child(
            div()
                .text_size(px(theme.typography.caption.size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.on_subtle)
                .child(label),
        )
        .child(input)
}

fn account_setup_status(status: &(String, bool), cx: &mut Context<AccountSetupModal>) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    div()
        .px(px(10.0))
        .py(px(8.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(if status.1 {
            colors.status_success
        } else {
            colors.status_error
        })
        .bg(colors.surface)
        .text_size(px(theme.typography.caption.size))
        .text_color(if status.1 {
            colors.status_success
        } else {
            colors.status_error
        })
        .child(status.0.clone())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SettingsTab {
    General,
    Account(String),
    Rules(String),
    Webhooks,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SettingsRuleMutation {
    Save(SettingsRuleSaveRequest),
    Toggle { rule_id: String },
    Delete { rule_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SettingsRuleActionDraft {
    action_type: String,
    action_value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SettingsRuleEditorMode {
    New,
    Edit(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SettingsRuleSaveRequest {
    mode: SettingsRuleEditorMode,
    account_id: String,
    name: String,
    match_mode: String,
    conditions: Vec<ConditionInput>,
    actions: Vec<ActionInput>,
}

#[derive(Clone)]
struct SettingsRuleConditionRow {
    key: u64,
    condition: Condition,
    value_input: Entity<TextInput>,
    field_dropdown: Entity<Dropdown>,
    operator_dropdown: Entity<Dropdown>,
    bool_dropdown: Entity<Dropdown>,
}

#[derive(Clone)]
struct SettingsRuleActionRow {
    key: u64,
    action_type: String,
    action_value: Option<String>,
    action_dropdown: Entity<Dropdown>,
    folder_dropdown: Entity<Dropdown>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SettingsWebhookMutation {
    Create(CreateWebhook),
    Toggle { webhook_id: String, enabled: bool },
    Delete { webhook_id: String },
}

#[derive(Clone)]
struct SettingsModal {
    state: Entity<NativeAppState>,
    focus_handle: FocusHandle,
    focused_on_open: bool,
    active_tab: SettingsTab,
    name_input: Entity<TextInput>,
    sync_days_input: Entity<TextInput>,
    signature_input: Entity<Textarea>,
    webhook_url_input: Entity<TextInput>,
    webhook_secret_input: Entity<TextInput>,
    webhook_events: String,
    webhooks: Vec<Webhook>,
    webhooks_loading: bool,
    webhook_busy_ids: BTreeSet<String>,
    adding_webhook: bool,
    rules_account_id: Option<String>,
    rules: Vec<RuleDto>,
    rules_loading: bool,
    rule_busy_ids: BTreeSet<String>,
    rule_editor_mode: Option<SettingsRuleEditorMode>,
    rule_editor_account_id: Option<String>,
    rule_name_input: Entity<TextInput>,
    rule_match_mode: MatchMode,
    rule_condition_rows: Vec<SettingsRuleConditionRow>,
    rule_action_rows: Vec<SettingsRuleActionRow>,
    rule_next_key: u64,
    rule_saving: bool,
    loading_account: bool,
    saving_account: bool,
    deleting_account: bool,
    confirm_delete_account: Option<String>,
    status: Option<(String, bool)>,
}

impl SettingsModal {
    fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            focus_handle: cx.focus_handle().tab_stop(true),
            focused_on_open: false,
            active_tab: SettingsTab::General,
            name_input: cx.new(|_| TextInput::new().placeholder("Display name")),
            sync_days_input: cx.new(|_| TextInput::new().placeholder("All")),
            signature_input: cx.new(|_| {
                Textarea::new()
                    .placeholder("Your email signature...")
                    .rows(5)
            }),
            webhook_url_input: cx
                .new(|_| TextInput::new().placeholder("https://example.com/webhook")),
            webhook_secret_input: cx.new(|_| TextInput::new().placeholder("HMAC signing secret")),
            webhook_events: "new_message".to_string(),
            webhooks: Vec::new(),
            webhooks_loading: false,
            webhook_busy_ids: BTreeSet::new(),
            adding_webhook: false,
            rules_account_id: None,
            rules: Vec::new(),
            rules_loading: false,
            rule_busy_ids: BTreeSet::new(),
            rule_editor_mode: None,
            rule_editor_account_id: None,
            rule_name_input: cx.new(|_| TextInput::new().placeholder("Rule name")),
            rule_match_mode: MatchMode::All,
            rule_condition_rows: Vec::new(),
            rule_action_rows: Vec::new(),
            rule_next_key: 1,
            rule_saving: false,
            loading_account: false,
            saving_account: false,
            deleting_account: false,
            confirm_delete_account: None,
            status: None,
        }
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        self.active_tab = SettingsTab::General;
        self.loading_account = false;
        self.saving_account = false;
        self.deleting_account = false;
        self.webhooks_loading = false;
        self.webhook_busy_ids.clear();
        self.adding_webhook = false;
        self.rules_account_id = None;
        self.rules_loading = false;
        self.rule_busy_ids.clear();
        self.reset_rule_editor(cx);
        self.confirm_delete_account = None;
        self.status = None;
        self.name_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.sync_days_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.signature_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.webhook_url_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.webhook_secret_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.webhook_events = "new_message".to_string();
        cx.notify();
    }

    fn select_account(&mut self, account: &Account, cx: &mut Context<Self>) {
        self.active_tab = SettingsTab::Account(account.id.clone());
        self.loading_account = true;
        self.saving_account = false;
        self.deleting_account = false;
        self.confirm_delete_account = None;
        self.webhook_busy_ids.clear();
        self.adding_webhook = false;
        self.rules_loading = false;
        self.rule_busy_ids.clear();
        self.reset_rule_editor(cx);
        self.status = None;
        self.name_input
            .update(cx, |input, cx| input.set_value(account.name.clone(), cx));
        self.sync_days_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.signature_input
            .update(cx, |input, cx| input.set_value("", cx));
        cx.notify();
    }

    fn apply_account_settings(&mut self, settings: AccountSettings, cx: &mut Context<Self>) {
        if self.active_tab != SettingsTab::Account(settings.id.clone()) {
            return;
        }

        self.loading_account = false;
        self.status = None;
        self.name_input
            .update(cx, |input, cx| input.set_value(settings.name, cx));
        self.sync_days_input.update(cx, |input, cx| {
            input.set_value(
                settings
                    .sync_days_limit
                    .map(|days| days.to_string())
                    .unwrap_or_default(),
                cx,
            );
        });
        self.signature_input.update(cx, |input, cx| {
            input.set_value(settings.signature.unwrap_or_default(), cx);
        });
        cx.notify();
    }

    fn fail_account_settings(&mut self, account_id: &str, message: String, cx: &mut Context<Self>) {
        if self.active_tab != SettingsTab::Account(account_id.to_string()) {
            return;
        }
        self.loading_account = false;
        self.status = Some((message, false));
        cx.notify();
    }

    fn set_status(&mut self, status: Option<(String, bool)>, cx: &mut Context<Self>) {
        self.status = status;
        self.saving_account = false;
        self.deleting_account = false;
        cx.notify();
    }

    fn select_webhooks(&mut self, cx: &mut Context<Self>) {
        self.active_tab = SettingsTab::Webhooks;
        self.webhooks_loading = true;
        self.webhook_busy_ids.clear();
        self.adding_webhook = false;
        self.status = None;
        cx.notify();
    }

    fn select_rules(&mut self, account_id: String, cx: &mut Context<Self>) {
        self.active_tab = SettingsTab::Rules(account_id.clone());
        self.rules_account_id = Some(account_id);
        self.rules_loading = true;
        self.rule_busy_ids.clear();
        self.reset_rule_editor(cx);
        self.status = None;
        cx.notify();
    }

    fn apply_rules(
        &mut self,
        account_id: &str,
        rules: Vec<RuleDto>,
        status: Option<(String, bool)>,
        cx: &mut Context<Self>,
    ) {
        if self.active_tab != SettingsTab::Rules(account_id.to_string()) {
            return;
        }
        self.rules_account_id = Some(account_id.to_string());
        self.rules = rules;
        self.rules_loading = false;
        self.rule_busy_ids.clear();
        self.status = status;
        cx.notify();
    }

    fn fail_rules(&mut self, account_id: &str, message: String, cx: &mut Context<Self>) {
        if self.active_tab != SettingsTab::Rules(account_id.to_string()) {
            return;
        }
        self.rules_loading = false;
        self.rule_busy_ids.clear();
        self.rule_saving = false;
        self.status = Some((message, false));
        cx.notify();
    }

    fn reset_rule_editor(&mut self, cx: &mut Context<Self>) {
        self.rule_editor_mode = None;
        self.rule_editor_account_id = None;
        self.rule_match_mode = MatchMode::All;
        self.rule_condition_rows.clear();
        self.rule_action_rows.clear();
        self.rule_next_key = 1;
        self.rule_saving = false;
        self.rule_name_input
            .update(cx, |input, cx| input.set_value("", cx));
    }

    fn start_new_rule_editor(
        &mut self,
        account_id: String,
        folders: Vec<Folder>,
        cx: &mut Context<Self>,
    ) {
        self.reset_rule_editor(cx);
        self.rule_editor_mode = Some(SettingsRuleEditorMode::New);
        self.rule_editor_account_id = Some(account_id);
        self.rule_match_mode = MatchMode::All;
        let condition_id = self.next_rule_condition_id();
        self.add_rule_condition(new_advanced_condition(condition_id), cx);
        self.add_rule_action("mark_read".to_string(), None, folders, cx);
        self.status = None;
        cx.notify();
    }

    fn start_edit_rule_editor(
        &mut self,
        account_id: String,
        rule: RuleDto,
        folders: Vec<Folder>,
        cx: &mut Context<Self>,
    ) {
        self.reset_rule_editor(cx);
        self.rule_editor_mode = Some(SettingsRuleEditorMode::Edit(rule.id));
        self.rule_editor_account_id = Some(account_id);
        self.rule_name_input
            .update(cx, |input, cx| input.set_value(rule.name, cx));
        self.rule_match_mode = match rule.match_mode.as_str() {
            "any" => MatchMode::Any,
            _ => MatchMode::All,
        };

        for condition in rule.conditions {
            let field = parse_condition_field(&condition.field).unwrap_or(ConditionField::From);
            let operator = parse_condition_operator(&condition.operator)
                .unwrap_or(ConditionOperator::Contains);
            self.add_rule_condition(
                Condition {
                    id: condition.id,
                    field,
                    operator,
                    value: condition.value,
                },
                cx,
            );
        }
        if self.rule_condition_rows.is_empty() {
            let condition_id = self.next_rule_condition_id();
            self.add_rule_condition(new_advanced_condition(condition_id), cx);
        }

        for action in rule.actions {
            self.add_rule_action(action.action_type, action.action_value, folders.clone(), cx);
        }
        if self.rule_action_rows.is_empty() {
            self.add_rule_action("mark_read".to_string(), None, folders, cx);
        }
        self.status = None;
        cx.notify();
    }

    fn next_rule_key(&mut self) -> u64 {
        let key = self.rule_next_key;
        self.rule_next_key = self.rule_next_key.saturating_add(1);
        key
    }

    fn next_rule_condition_id(&mut self) -> String {
        format!("settings-rule-condition-{}", self.next_rule_key())
    }

    fn add_rule_condition(&mut self, condition: Condition, cx: &mut Context<Self>) {
        let key = self.next_rule_key();
        let condition_id = condition.id.clone();
        let initial_value = condition.value.clone();
        let modal_entity = cx.entity();
        let value_input = cx.new(move |_| {
            TextInput::new()
                .value(initial_value.clone())
                .placeholder("Value")
                .on_change({
                    let modal_entity = modal_entity.clone();
                    let condition_id = condition_id.clone();
                    move |value, app_cx| {
                        let _ = modal_entity.update(app_cx, |modal, cx| {
                            modal.set_rule_condition_value(&condition_id, value.to_string());
                            cx.notify();
                        });
                    }
                })
        });
        let field_condition_id = condition.id.clone();
        let field_modal = cx.entity();
        let field_selected = condition_field_value(&condition.field);
        let field_dropdown = cx.new(move |_| {
            rule_condition_field_dropdown(
                ("settings-rule-field", key),
                field_selected,
                field_modal.clone(),
                field_condition_id.clone(),
            )
        });
        let operator_condition_id = condition.id.clone();
        let operator_modal = cx.entity();
        let operator_selected = condition_operator_value(&condition.operator);
        let operator_dropdown = cx.new(move |_| {
            rule_condition_operator_dropdown(
                ("settings-rule-op", key),
                operator_selected,
                operator_modal.clone(),
                operator_condition_id.clone(),
            )
        });
        let bool_condition_id = condition.id.clone();
        let bool_modal = cx.entity();
        let bool_selected = if condition.value.trim() == "false" {
            "false"
        } else {
            "true"
        };
        let bool_dropdown = cx.new(move |_| {
            rule_boolean_dropdown(
                ("settings-rule-bool", key),
                bool_selected,
                bool_modal.clone(),
                bool_condition_id.clone(),
            )
        });
        self.rule_condition_rows.push(SettingsRuleConditionRow {
            key,
            condition,
            value_input,
            field_dropdown,
            operator_dropdown,
            bool_dropdown,
        });
    }

    fn remove_rule_condition(&mut self, condition_id: &str, cx: &mut Context<Self>) {
        self.rule_condition_rows
            .retain(|row| row.condition.id != condition_id);
        if self.rule_condition_rows.is_empty() {
            let id = self.next_rule_condition_id();
            self.add_rule_condition(new_advanced_condition(id), cx);
        }
        cx.notify();
    }

    fn set_rule_condition_field(
        &mut self,
        condition_id: &str,
        field: ConditionField,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self
            .rule_condition_rows
            .iter_mut()
            .find(|row| row.condition.id == condition_id)
        {
            row.condition.field = field;
            row.condition.operator = ConditionOperator::Contains;
            row.condition.value = default_condition_value(&row.condition.field).to_string();
            let value = row.condition.value.clone();
            let operator = row.condition.operator.clone();
            let bool_value = if row.condition.value == "false" {
                "false"
            } else {
                "true"
            };
            row.value_input.update(cx, |input, cx| {
                input.set_value(value, cx);
            });
            row.operator_dropdown.update(cx, |dropdown, cx| {
                dropdown.set_selected(condition_operator_value(&operator), cx);
            });
            row.bool_dropdown.update(cx, |dropdown, cx| {
                dropdown.set_selected(bool_value, cx);
            });
        }
        cx.notify();
    }

    fn set_rule_condition_operator(&mut self, condition_id: &str, operator: ConditionOperator) {
        if let Some(row) = self
            .rule_condition_rows
            .iter_mut()
            .find(|row| row.condition.id == condition_id)
        {
            row.condition.operator = operator;
        }
    }

    fn set_rule_condition_value(&mut self, condition_id: &str, value: String) {
        if let Some(row) = self
            .rule_condition_rows
            .iter_mut()
            .find(|row| row.condition.id == condition_id)
        {
            row.condition.value = value;
        }
    }

    fn add_rule_action(
        &mut self,
        action_type: String,
        action_value: Option<String>,
        folders: Vec<Folder>,
        cx: &mut Context<Self>,
    ) {
        let key = self.next_rule_key();
        let modal_entity = cx.entity();
        let action_dropdown = cx.new({
            let action_type = action_type.clone();
            let modal_entity = modal_entity.clone();
            move |_| {
                rule_action_type_dropdown(
                    ("settings-rule-action-type", key),
                    action_type.as_str(),
                    modal_entity.clone(),
                    key,
                )
            }
        });
        let folder_dropdown = cx.new({
            let selected = action_value.clone();
            let modal_entity = cx.entity();
            move |_| {
                rule_action_folder_dropdown(
                    ("settings-rule-action-folder", key),
                    selected.clone(),
                    folders.clone(),
                    modal_entity.clone(),
                    key,
                )
            }
        });
        self.rule_action_rows.push(SettingsRuleActionRow {
            key,
            action_type,
            action_value,
            action_dropdown,
            folder_dropdown,
        });
        cx.notify();
    }

    fn remove_rule_action(&mut self, key: u64, cx: &mut Context<Self>) {
        if self.rule_action_rows.len() <= 1 {
            cx.notify();
            return;
        }
        self.rule_action_rows.retain(|row| row.key != key);
        cx.notify();
    }

    fn set_rule_action_type(&mut self, key: u64, action_type: String) {
        if let Some(row) = self.rule_action_rows.iter_mut().find(|row| row.key == key) {
            row.action_type = action_type;
            if row.action_type != "move_to_folder" {
                row.action_value = None;
            }
        }
    }

    fn set_rule_action_value(&mut self, key: u64, action_value: Option<String>) {
        if let Some(row) = self.rule_action_rows.iter_mut().find(|row| row.key == key) {
            row.action_value = action_value;
        }
    }

    fn set_rule_match_mode(&mut self, match_mode: MatchMode) {
        self.rule_match_mode = match_mode;
    }

    fn apply_webhooks(
        &mut self,
        webhooks: Vec<Webhook>,
        status: Option<(String, bool)>,
        clear_new: bool,
        cx: &mut Context<Self>,
    ) {
        if self.active_tab != SettingsTab::Webhooks {
            return;
        }
        self.webhooks = webhooks;
        self.webhooks_loading = false;
        self.webhook_busy_ids.clear();
        self.adding_webhook = false;
        self.status = status;
        if clear_new {
            self.webhook_url_input
                .update(cx, |input, cx| input.set_value("", cx));
            self.webhook_secret_input
                .update(cx, |input, cx| input.set_value("", cx));
            self.webhook_events = "new_message".to_string();
        }
        cx.notify();
    }

    fn fail_webhooks(&mut self, message: String, cx: &mut Context<Self>) {
        if self.active_tab != SettingsTab::Webhooks {
            return;
        }
        self.webhooks_loading = false;
        self.webhook_busy_ids.clear();
        self.adding_webhook = false;
        self.status = Some((message, false));
        cx.notify();
    }
}

impl Render for SettingsModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        if !state.modals.settings_open {
            self.focused_on_open = false;
            return div();
        }
        if !self.focused_on_open {
            window.focus(&self.focus_handle);
            self.focused_on_open = true;
        }

        let theme = cx.theme();
        let colors = theme.colors.clone();
        let mut backdrop = colors.surface_dim;
        backdrop.a = 0.58;
        let modal_entity = cx.entity();

        div()
            .absolute()
            .inset_0()
            .size_full()
            .tab_index(0)
            .key_context("SettingsModal")
            .track_focus(&self.focus_handle)
            .flex()
            .items_center()
            .justify_center()
            .bg(backdrop)
            .on_key_down({
                let state_entity = self.state.clone();
                let modal_entity = modal_entity.clone();
                move |ev: &KeyDownEvent, _, app_cx| {
                    if handle_settings_modal_key(
                        &state_entity,
                        &modal_entity,
                        ev.keystroke.key.as_str(),
                        ev.keystroke.modifiers.control || ev.keystroke.modifiers.platform,
                        app_cx,
                    ) {
                        app_cx.stop_propagation();
                    }
                }
            })
            .on_mouse_down(MouseButton::Left, {
                let state_entity = self.state.clone();
                move |_, _, app_cx| close_settings_modal(&state_entity, app_cx)
            })
            .child(
                div()
                    .w(px(860.0))
                    .max_h(px(620.0))
                    .flex()
                    .flex_col()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(colors.stroke_neutral)
                    .bg(colors.surface)
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, app_cx| {
                        app_cx.stop_propagation();
                    })
                    .child(settings_header(&self.state, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .child(settings_tabs(&self.state, &modal_entity, &state, self, cx))
                            .child(settings_body(&self.state, &modal_entity, &state, self, cx)),
                    ),
            )
    }
}

fn settings_header(
    state_entity: &Entity<NativeAppState>,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    div()
        .px(px(18.0))
        .py(px(14.0))
        .border_b_1()
        .border_color(colors.stroke_neutral_subtle)
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(px(typography.subtitle.size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.on_neutral)
                .child("Settings"),
        )
        .child(
            Button::new("settings-close")
                .label("Close")
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .on_click({
                    let state_entity = state_entity.clone();
                    move |_, _, app_cx| close_settings_modal(&state_entity, app_cx)
                }),
        )
}

fn settings_tabs(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    state: &NativeAppState,
    modal: &SettingsModal,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let mut tabs = div()
        .w(px(220.0))
        .min_h_0()
        .border_r_1()
        .border_color(colors.stroke_neutral_subtle)
        .p(px(10.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(settings_tab_button(
            "settings-tab-general",
            "General",
            modal.active_tab == SettingsTab::General,
            {
                let modal_entity = modal_entity.clone();
                move |_, _, app_cx| {
                    let _ = modal_entity.update(app_cx, |modal, cx| {
                        modal.active_tab = SettingsTab::General;
                        modal.status = None;
                        cx.notify();
                    });
                }
            },
            cx,
        ));

    for account in &state.accounts {
        tabs = tabs.child(settings_tab_button(
            SharedString::from(format!("settings-tab-account-{}", account.id)),
            account.name.as_str(),
            modal.active_tab == SettingsTab::Account(account.id.clone()),
            {
                let state_entity = state_entity.clone();
                let modal_entity = modal_entity.clone();
                let account_id = account.id.clone();
                move |_, _, app_cx| {
                    select_settings_account(
                        &state_entity,
                        &modal_entity,
                        account_id.clone(),
                        app_cx,
                    );
                }
            },
            cx,
        ));
    }

    for account in &state.accounts {
        let label = format!("Rules ({})", account.name);
        tabs = tabs.child(settings_tab_button(
            SharedString::from(format!("settings-tab-rules-{}", account.id)),
            label.as_str(),
            modal.active_tab == SettingsTab::Rules(account.id.clone()),
            {
                let state_entity = state_entity.clone();
                let modal_entity = modal_entity.clone();
                let account_id = account.id.clone();
                move |_, _, app_cx| {
                    select_settings_rules(&state_entity, &modal_entity, account_id.clone(), app_cx);
                }
            },
            cx,
        ));
    }

    tabs.child(settings_tab_button(
        "settings-tab-webhooks",
        "Webhooks",
        modal.active_tab == SettingsTab::Webhooks,
        {
            let state_entity = state_entity.clone();
            let modal_entity = modal_entity.clone();
            move |_, _, app_cx| {
                select_settings_webhooks(&state_entity, &modal_entity, app_cx);
            }
        },
        cx,
    ))
}

fn settings_tab_button(
    id: impl Into<gpui::ElementId>,
    label: &str,
    selected: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    cx: &mut Context<SettingsModal>,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    div()
        .id(id)
        .h(px(32.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .bg(if selected {
            colors.neutral_selected
        } else {
            colors.surface
        })
        .text_size(px(theme.typography.body.size))
        .font_weight(if selected {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        })
        .text_color(colors.on_neutral)
        .cursor_pointer()
        .on_click(on_click)
        .child(label.to_string())
        .into_any_element()
}

fn settings_body(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    state: &NativeAppState,
    modal: &SettingsModal,
    cx: &mut Context<SettingsModal>,
) -> gpui::AnyElement {
    match &modal.active_tab {
        SettingsTab::General => div()
            .id("settings-body-general")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(18.0))
            .child(settings_general_body(state, cx))
            .into_any_element(),
        SettingsTab::Account(account_id) => div()
            .id("settings-body-account")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(18.0))
            .child(settings_account_body(
                state_entity,
                modal_entity,
                state,
                modal,
                account_id,
                cx,
            ))
            .into_any_element(),
        SettingsTab::Rules(account_id) => div()
            .id("settings-body-rules")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(18.0))
            .child(settings_rules_body(
                state,
                modal_entity,
                modal,
                account_id,
                cx,
            ))
            .into_any_element(),
        SettingsTab::Webhooks => div()
            .id("settings-body-webhooks")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(18.0))
            .child(settings_webhooks_body(modal_entity, modal, cx))
            .into_any_element(),
    }
}

fn settings_general_body(state: &NativeAppState, cx: &mut Context<SettingsModal>) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let theme_is_dark = state.theme == ThemeMode::Dark;
    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(settings_section_title("Appearance", cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Theme", cx))
                .child(
                    Button::new("settings-theme-light")
                        .label("Light")
                        .appearance(if theme_is_dark {
                            ButtonAppearance::Subtle
                        } else {
                            ButtonAppearance::Neutral
                        })
                        .size(ButtonSize::Compact)
                        .on_click(|_, _, app_cx| app_cx.dispatch_action(&ApplyLightTheme)),
                )
                .child(
                    Button::new("settings-theme-dark")
                        .label("Dark")
                        .appearance(if theme_is_dark {
                            ButtonAppearance::Neutral
                        } else {
                            ButtonAppearance::Subtle
                        })
                        .size(ButtonSize::Compact)
                        .on_click(|_, _, app_cx| app_cx.dispatch_action(&ApplyDarkTheme)),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Density", cx))
                .child(
                    IconButton::new("settings-density-down", "icons/chevron_down.svg")
                        .size(ButtonSize::Compact)
                        .disabled(state.density.level == 0)
                        .on_click(|_, _, app_cx| app_cx.dispatch_action(&DecreaseDensity)),
                )
                .child(
                    div()
                        .w(px(96.0))
                        .text_size(px(typography.body.size))
                        .text_color(colors.on_neutral)
                        .child(format!("Level {}", state.density.level)),
                )
                .child(
                    IconButton::new("settings-density-up", "icons/chevron_up.svg")
                        .size(ButtonSize::Compact)
                        .disabled(state.density.level >= 8)
                        .on_click(|_, _, app_cx| app_cx.dispatch_action(&IncreaseDensity)),
                ),
        )
}

fn settings_account_body(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    state: &NativeAppState,
    modal: &SettingsModal,
    account_id: &str,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let Some(account) = state
        .accounts
        .iter()
        .find(|account| account.id == account_id)
    else {
        return div()
            .text_size(px(typography.body.size))
            .text_color(colors.on_subtle)
            .child("Account not found.");
    };
    let folders = state
        .folders_by_account
        .get(account_id)
        .cloned()
        .unwrap_or_default();
    let status = modal.status.clone();
    let confirming_delete = modal.confirm_delete_account.as_deref() == Some(account_id);
    let account_email = account.email.clone();

    let mut folder_list = div().flex().flex_col().gap(px(4.0));
    if folders.is_empty() {
        folder_list = folder_list.child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("No folders synced yet."),
        );
    } else {
        for folder in folders {
            let folder_id = folder.id.clone();
            let next_excluded = !folder.is_excluded;
            folder_list = folder_list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Checkbox::new(SharedString::from(format!("settings-folder-{}", folder.id)))
                            .state(if folder.is_excluded {
                                CheckboxState::Unchecked
                            } else {
                                CheckboxState::Checked
                            })
                            .on_click({
                                let state_entity = state_entity.clone();
                                let modal_entity = modal_entity.clone();
                                move |_, _, _, app_cx| {
                                    start_folder_exclusion_toggle(
                                        &state_entity,
                                        &modal_entity,
                                        folder_id.clone(),
                                        next_excluded,
                                        app_cx,
                                    );
                                }
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(typography.body.size))
                            .text_color(colors.on_neutral)
                            .child(folder_label(&folder)),
                    )
                    .when(folder.is_excluded, |row| {
                        row.child(
                            div()
                                .text_size(px(typography.caption.size))
                                .text_color(colors.on_subtle)
                                .child("excluded"),
                        )
                    }),
            );
        }
    }

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(settings_section_title("Account info", cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Display name", cx))
                .child(div().w(px(260.0)).child(modal.name_input.clone())),
        )
        .child(settings_readonly_row("Email", account.email.as_str(), cx))
        .child(settings_readonly_row(
            "Provider",
            account.provider_type.as_str(),
            cx,
        ))
        .child(settings_section_title("Sync settings", cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Download days", cx))
                .child(div().w(px(140.0)).child(modal.sync_days_input.clone()))
                .child(
                    div()
                        .text_size(px(typography.caption.size))
                        .text_color(colors.on_subtle)
                        .child("Leave empty for all"),
                ),
        )
        .child(settings_section_title("Signature", cx))
        .child(modal.signature_input.clone())
        .child(settings_section_title("Excluded folders", cx))
        .child(folder_list)
        .when(modal.loading_account, |body| {
            body.child(
                div()
                    .text_size(px(typography.caption.size))
                    .text_color(colors.on_subtle)
                    .child("Loading account settings..."),
            )
        })
        .when_some(status, |body, (message, ok)| {
            body.child(
                div()
                    .text_size(px(typography.caption.size))
                    .text_color(if ok {
                        colors.status_success
                    } else {
                        colors.status_error
                    })
                    .child(message),
            )
        })
        .child(
            div().flex().items_center().gap(px(8.0)).child(
                Button::new("settings-save-account")
                    .label(if modal.saving_account {
                        "Saving..."
                    } else {
                        "Save changes"
                    })
                    .appearance(ButtonAppearance::Accent)
                    .size(ButtonSize::Compact)
                    .disabled(modal.loading_account || modal.saving_account)
                    .on_click({
                        let state_entity = state_entity.clone();
                        let modal_entity = modal_entity.clone();
                        let account_id = account_id.to_string();
                        move |_, _, app_cx| {
                            start_settings_save(
                                &state_entity,
                                &modal_entity,
                                account_id.clone(),
                                app_cx,
                            );
                        }
                    }),
            ),
        )
        .child(settings_section_title("Danger zone", cx))
        .child(if confirming_delete {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(typography.body.size))
                        .text_color(colors.on_neutral)
                        .child(format!("Delete {account_email} and all its cached emails?")),
                )
                .child(
                    Button::new("settings-confirm-delete-account")
                        .label(if modal.deleting_account {
                            "Deleting..."
                        } else {
                            "Yes, delete"
                        })
                        .appearance(ButtonAppearance::Danger)
                        .size(ButtonSize::Compact)
                        .disabled(
                            modal.loading_account || modal.saving_account || modal.deleting_account,
                        )
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            let account_id = account_id.to_string();
                            move |_, _, app_cx| {
                                start_settings_account_delete(
                                    &state_entity,
                                    &modal_entity,
                                    account_id.clone(),
                                    app_cx,
                                );
                            }
                        }),
                )
                .child(
                    Button::new("settings-cancel-delete-account")
                        .label("Cancel")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .disabled(modal.deleting_account)
                        .on_click({
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                cancel_settings_account_delete(&modal_entity, app_cx);
                            }
                        }),
                )
                .into_any_element()
        } else {
            Button::new("settings-remove-account")
                .label("Remove account")
                .appearance(ButtonAppearance::Danger)
                .size(ButtonSize::Compact)
                .disabled(modal.loading_account || modal.saving_account || modal.deleting_account)
                .on_click({
                    let modal_entity = modal_entity.clone();
                    let account_id = account_id.to_string();
                    move |_, _, app_cx| {
                        confirm_settings_account_delete(&modal_entity, account_id.clone(), app_cx);
                    }
                })
                .into_any_element()
        })
}

fn settings_rules_body(
    state: &NativeAppState,
    modal_entity: &Entity<SettingsModal>,
    modal: &SettingsModal,
    account_id: &str,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let status = modal.status.clone();
    let state_entity = modal.state.clone();
    let account_label = state
        .accounts
        .iter()
        .find(|account| account.id == account_id)
        .map(|account| account.name.clone())
        .unwrap_or_else(|| "Account".to_string());
    let folders = state
        .folders_by_account
        .get(account_id)
        .cloned()
        .unwrap_or_default();
    let editor_open = modal.rule_editor_mode.is_some()
        && modal.rule_editor_account_id.as_deref() == Some(account_id);

    let mut rule_list = div().flex().flex_col().gap(px(6.0));
    if modal.rules_loading {
        rule_list = rule_list.child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("Loading rules..."),
        );
    } else if modal.rules_account_id.as_deref() != Some(account_id) {
        rule_list = rule_list.child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("Select this tab to load rules."),
        );
    } else if modal.rules.is_empty() {
        rule_list = rule_list.child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("No rules configured for this account."),
        );
    } else {
        for rule in &modal.rules {
            let rule_id = rule.id.clone();
            let edit_rule = rule.clone();
            let busy = modal.rule_busy_ids.contains(&rule.id);
            rule_list = rule_list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(colors.stroke_neutral_subtle)
                    .child(
                        Checkbox::new(SharedString::from(format!(
                            "settings-rule-active-{}",
                            rule.id
                        )))
                        .checked(rule.is_active)
                        .disabled(busy || modal.rules_loading)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            let account_id = account_id.to_string();
                            let rule_id = rule_id.clone();
                            move |_, _, _, app_cx| {
                                start_settings_rule_toggle(
                                    &state_entity,
                                    &modal_entity,
                                    account_id.clone(),
                                    rule_id.clone(),
                                    app_cx,
                                );
                            }
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(typography.body.size))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.on_neutral)
                                    .child(rule.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(typography.caption.size))
                                    .text_color(colors.on_subtle)
                                    .child(rule_counts_text(rule)),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "settings-rule-edit-{}",
                            rule.id
                        )))
                        .label("Edit")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .disabled(busy || modal.rules_loading)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            let account_id = account_id.to_string();
                            let edit_rule = edit_rule.clone();
                            move |_, _, app_cx| {
                                start_settings_rule_edit(
                                    &state_entity,
                                    &modal_entity,
                                    account_id.clone(),
                                    edit_rule.clone(),
                                    app_cx,
                                );
                            }
                        }),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "settings-rule-delete-{}",
                            rule.id
                        )))
                        .label("Delete")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .disabled(busy || modal.rules_loading)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            let account_id = account_id.to_string();
                            let rule_id = rule_id.clone();
                            move |_, _, app_cx| {
                                start_settings_rule_delete(
                                    &state_entity,
                                    &modal_entity,
                                    account_id.clone(),
                                    rule_id.clone(),
                                    app_cx,
                                );
                            }
                        }),
                    ),
            );
        }
    }

    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(settings_section_title("Rules", cx))
                .when(!editor_open, |row| {
                    row.child(
                        Button::new("settings-new-rule")
                            .label("New rule")
                            .appearance(ButtonAppearance::Accent)
                            .size(ButtonSize::Compact)
                            .disabled(modal.rules_loading)
                            .on_click({
                                let state_entity = state_entity.clone();
                                let modal_entity = modal_entity.clone();
                                let account_id = account_id.to_string();
                                move |_, _, app_cx| {
                                    start_settings_rule_new(
                                        &state_entity,
                                        &modal_entity,
                                        account_id.clone(),
                                        app_cx,
                                    );
                                }
                            }),
                    )
                }),
        )
        .child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(colors.on_subtle)
                .child(account_label),
        );

    if editor_open {
        body = body.child(settings_rule_editor_body(modal_entity, modal, folders, cx));
    } else {
        body = body.child(rule_list);
    }

    body.when_some(status, |body, (message, ok)| {
        body.child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(if ok {
                    colors.status_success
                } else {
                    colors.status_error
                })
                .child(message),
        )
    })
}

fn settings_rule_editor_body(
    modal_entity: &Entity<SettingsModal>,
    modal: &SettingsModal,
    folders: Vec<Folder>,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let state_entity = modal.state.clone();
    let modal_entity_for_rows = modal_entity.clone();
    let title = match modal.rule_editor_mode {
        Some(SettingsRuleEditorMode::New) => "New rule",
        Some(SettingsRuleEditorMode::Edit(_)) => "Edit rule",
        None => "Rule",
    };

    let mut condition_rows = div().flex().flex_col().gap(px(8.0));
    for row in modal.rule_condition_rows.clone() {
        condition_rows = condition_rows.child(settings_rule_condition_row(
            row,
            modal_entity_for_rows.clone(),
            cx,
        ));
    }

    let mut action_rows = div().flex().flex_col().gap(px(8.0));
    let can_remove_action = modal.rule_action_rows.len() > 1;
    for row in modal.rule_action_rows.clone() {
        action_rows = action_rows.child(settings_rule_action_row(
            row,
            modal_entity.clone(),
            can_remove_action,
            cx,
        ));
    }

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .text_size(px(typography.body.size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.on_neutral)
                .child(title),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Name", cx))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(240.0))
                        .child(modal.rule_name_input.clone()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Match", cx))
                .child(
                    Button::new("settings-rule-match-all")
                        .label("All")
                        .appearance(if modal.rule_match_mode == MatchMode::All {
                            ButtonAppearance::Neutral
                        } else {
                            ButtonAppearance::Subtle
                        })
                        .size(ButtonSize::Compact)
                        .on_click({
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                let _ = modal_entity.update(app_cx, |modal, cx| {
                                    modal.set_rule_match_mode(MatchMode::All);
                                    cx.notify();
                                });
                            }
                        }),
                )
                .child(
                    Button::new("settings-rule-match-any")
                        .label("Any")
                        .appearance(if modal.rule_match_mode == MatchMode::Any {
                            ButtonAppearance::Neutral
                        } else {
                            ButtonAppearance::Subtle
                        })
                        .size(ButtonSize::Compact)
                        .on_click({
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                let _ = modal_entity.update(app_cx, |modal, cx| {
                                    modal.set_rule_match_mode(MatchMode::Any);
                                    cx.notify();
                                });
                            }
                        }),
                )
                .child(
                    div()
                        .text_size(px(typography.caption.size))
                        .text_color(colors.on_subtle)
                        .child("of the following conditions"),
                ),
        )
        .child(settings_section_title("Conditions", cx))
        .child(condition_rows)
        .child(
            Button::new("settings-rule-add-condition")
                .label("Add condition")
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .disabled(modal.rule_saving)
                .on_click({
                    let modal_entity = modal_entity.clone();
                    move |_, _, app_cx| {
                        let _ = modal_entity.update(app_cx, |modal, cx| {
                            let id = modal.next_rule_condition_id();
                            modal.add_rule_condition(new_advanced_condition(id), cx);
                        });
                    }
                }),
        )
        .child(settings_section_title("Actions", cx))
        .child(action_rows)
        .child(
            Button::new("settings-rule-add-action")
                .label("Add action")
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .disabled(modal.rule_saving)
                .on_click({
                    let modal_entity = modal_entity.clone();
                    let folders = folders.clone();
                    move |_, _, app_cx| {
                        let _ = modal_entity.update(app_cx, |modal, cx| {
                            modal.add_rule_action(
                                "mark_read".to_string(),
                                None,
                                folders.clone(),
                                cx,
                            );
                        });
                    }
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(div().flex_1())
                .child(
                    Button::new("settings-rule-cancel")
                        .label("Cancel")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .disabled(modal.rule_saving)
                        .on_click({
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                cancel_settings_rule_editor(&modal_entity, app_cx);
                            }
                        }),
                )
                .child(
                    Button::new("settings-rule-save")
                        .label(if modal.rule_saving {
                            "Saving..."
                        } else {
                            "Save rule"
                        })
                        .appearance(ButtonAppearance::Accent)
                        .size(ButtonSize::Compact)
                        .disabled(modal.rule_saving || modal.rules_loading)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                start_settings_rule_save(&state_entity, &modal_entity, app_cx);
                            }
                        }),
                ),
        )
}

fn settings_rule_condition_row(
    row: SettingsRuleConditionRow,
    modal_entity: Entity<SettingsModal>,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let field_type = condition_field_type(&row.condition.field);
    let condition_id = row.condition.id.clone();
    let mut line = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(div().w(px(170.0)).child(row.field_dropdown.clone()));

    match field_type {
        ConditionFieldType::Text => {
            line = line
                .child(div().w(px(170.0)).child(row.operator_dropdown.clone()))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(180.0))
                        .child(row.value_input.clone()),
                );
        }
        ConditionFieldType::Boolean => {
            line = line
                .child(div().w(px(170.0)).child(row.bool_dropdown.clone()))
                .child(div().flex_1());
        }
        ConditionFieldType::Date => {
            line = line
                .child(
                    div()
                        .w(px(170.0))
                        .text_size(px(theme.typography.caption.size))
                        .text_color(colors.on_subtle)
                        .child("YYYY-MM-DD"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(180.0))
                        .child(row.value_input.clone()),
                );
        }
    }

    line.child(
        Button::new(("settings-rule-remove-condition", row.key))
            .label("Remove")
            .appearance(ButtonAppearance::Subtle)
            .size(ButtonSize::Compact)
            .on_click({
                let modal_entity = modal_entity.clone();
                move |_, _, app_cx| {
                    let _ = modal_entity.update(app_cx, |modal, cx| {
                        modal.remove_rule_condition(&condition_id, cx);
                    });
                }
            }),
    )
}

fn settings_rule_action_row(
    row: SettingsRuleActionRow,
    modal_entity: Entity<SettingsModal>,
    can_remove: bool,
    _cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let key = row.key;
    let mut line = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(div().w(px(190.0)).child(row.action_dropdown.clone()));

    if rule_action_needs_folder(&row.action_type) {
        line = line.child(
            div()
                .flex_1()
                .min_w(px(220.0))
                .child(row.folder_dropdown.clone()),
        );
    } else {
        line = line.child(div().flex_1());
    }

    line.child(
        Button::new(("settings-rule-remove-action", key))
            .label("Remove")
            .appearance(ButtonAppearance::Subtle)
            .size(ButtonSize::Compact)
            .disabled(!can_remove)
            .on_click({
                let modal_entity = modal_entity.clone();
                move |_, _, app_cx| {
                    let _ = modal_entity.update(app_cx, |modal, cx| {
                        modal.remove_rule_action(key, cx);
                    });
                }
            }),
    )
}

fn settings_webhooks_body(
    modal_entity: &Entity<SettingsModal>,
    modal: &SettingsModal,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let state_entity = modal.state.clone();
    let status = modal.status.clone();

    let mut webhook_list = div().flex().flex_col().gap(px(6.0));
    if modal.webhooks_loading {
        webhook_list = webhook_list.child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("Loading webhooks..."),
        );
    } else if modal.webhooks.is_empty() {
        webhook_list = webhook_list.child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("No webhooks configured."),
        );
    } else {
        for webhook in &modal.webhooks {
            let webhook_id = webhook.id.clone();
            let busy = modal.webhook_busy_ids.contains(&webhook.id);
            webhook_list = webhook_list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(colors.stroke_neutral_subtle)
                    .child(
                        Checkbox::new(SharedString::from(format!(
                            "settings-webhook-enabled-{}",
                            webhook.id
                        )))
                        .checked(webhook.enabled)
                        .disabled(busy || modal.webhooks_loading)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            let webhook_id = webhook_id.clone();
                            move |new_state, _, _, app_cx| {
                                start_settings_webhook_toggle(
                                    &state_entity,
                                    &modal_entity,
                                    webhook_id.clone(),
                                    matches!(new_state, CheckboxState::Checked),
                                    app_cx,
                                );
                            }
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(typography.body.size))
                                    .text_color(colors.on_neutral)
                                    .child(webhook.url.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(typography.caption.size))
                                    .text_color(colors.on_subtle)
                                    .child(format!(
                                        "Events: {}{}",
                                        webhook.events,
                                        if webhook.secret.is_some() {
                                            " - signed"
                                        } else {
                                            ""
                                        }
                                    )),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "settings-webhook-delete-{}",
                            webhook.id
                        )))
                        .label("Delete")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .disabled(busy || modal.webhooks_loading)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let modal_entity = modal_entity.clone();
                            let webhook_id = webhook_id.clone();
                            move |_, _, app_cx| {
                                start_settings_webhook_delete(
                                    &state_entity,
                                    &modal_entity,
                                    webhook_id.clone(),
                                    app_cx,
                                );
                            }
                        }),
                    ),
            );
        }
    }

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(settings_section_title("Configured webhooks", cx))
        .child(webhook_list)
        .child(settings_section_title("Add webhook", cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("URL", cx))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(220.0))
                        .child(modal.webhook_url_input.clone()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Secret", cx))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(220.0))
                        .child(modal.webhook_secret_input.clone()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_label("Events", cx))
                .child(
                    Button::new("settings-webhook-events-new-message")
                        .label("New message")
                        .appearance(if modal.webhook_events == "new_message" {
                            ButtonAppearance::Neutral
                        } else {
                            ButtonAppearance::Subtle
                        })
                        .size(ButtonSize::Compact)
                        .on_click({
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                let _ = modal_entity.update(app_cx, |modal, cx| {
                                    modal.webhook_events = "new_message".to_string();
                                    cx.notify();
                                });
                            }
                        }),
                )
                .child(
                    Button::new("settings-webhook-events-all")
                        .label("All")
                        .appearance(if modal.webhook_events == "all" {
                            ButtonAppearance::Neutral
                        } else {
                            ButtonAppearance::Subtle
                        })
                        .size(ButtonSize::Compact)
                        .on_click({
                            let modal_entity = modal_entity.clone();
                            move |_, _, app_cx| {
                                let _ = modal_entity.update(app_cx, |modal, cx| {
                                    modal.webhook_events = "all".to_string();
                                    cx.notify();
                                });
                            }
                        }),
                ),
        )
        .when_some(status, |body, (message, ok)| {
            body.child(
                div()
                    .text_size(px(typography.caption.size))
                    .text_color(if ok {
                        colors.status_success
                    } else {
                        colors.status_error
                    })
                    .child(message),
            )
        })
        .child(
            Button::new("settings-add-webhook")
                .label(if modal.adding_webhook {
                    "Adding..."
                } else {
                    "Add webhook"
                })
                .appearance(ButtonAppearance::Accent)
                .size(ButtonSize::Compact)
                .disabled(modal.adding_webhook || modal.webhooks_loading)
                .on_click({
                    let state_entity = state_entity.clone();
                    let modal_entity = modal_entity.clone();
                    move |_, _, app_cx| {
                        start_settings_webhook_create(&state_entity, &modal_entity, app_cx);
                    }
                }),
        )
}

fn settings_section_title(label: &'static str, cx: &mut Context<SettingsModal>) -> gpui::Div {
    let theme = cx.theme();
    div()
        .pt(px(4.0))
        .text_size(px(theme.typography.body.size))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.colors.on_neutral)
        .child(label)
}

fn settings_label(label: &'static str, cx: &mut Context<SettingsModal>) -> gpui::Div {
    let theme = cx.theme();
    div()
        .w(px(120.0))
        .text_size(px(theme.typography.caption.size))
        .text_color(theme.colors.on_subtle)
        .child(label)
}

fn settings_readonly_row(
    label: &'static str,
    value: &str,
    cx: &mut Context<SettingsModal>,
) -> gpui::Div {
    let theme = cx.theme();
    let body_size = theme.typography.body.size;
    let value_color = theme.colors.on_subtle;
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(settings_label(label, cx))
        .child(
            div()
                .text_size(px(body_size))
                .text_color(value_color)
                .child(value.to_string()),
        )
}

fn open_account_setup_modal(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    app_cx: &mut App,
) {
    state_entity.update(app_cx, |state, cx| {
        state.modals.account_setup_open = true;
        state.modals.settings_open = false;
        state.modals.advanced_search_open = false;
        cx.notify();
    });
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.reset(cx);
    });
}

fn close_account_setup_modal(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        if !state.accounts.is_empty() {
            state.modals.account_setup_open = false;
        }
        cx.notify();
    });
}

fn handle_account_setup_key(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    key: &str,
    app_cx: &mut App,
) -> bool {
    match key {
        "escape" => {
            let (mode, busy, first_run) = {
                let modal = modal_entity.read(app_cx);
                (
                    modal.mode,
                    modal.is_busy(),
                    modal.state.read(app_cx).accounts.is_empty(),
                )
            };
            if busy {
                return true;
            }
            if mode != AccountSetupMode::Choice {
                let _ = modal_entity.update(app_cx, |modal, cx| {
                    modal.show_mode(AccountSetupMode::Choice, cx);
                });
                return true;
            }
            if !first_run {
                close_account_setup_modal(state_entity, app_cx);
            }
            true
        }
        "enter" | "return" => {
            let mode = modal_entity.read(app_cx).mode;
            if mode == AccountSetupMode::ManualImap
                || mode == AccountSetupMode::MicrosoftAppPassword
            {
                start_account_setup_create(state_entity, modal_entity, app_cx);
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

fn cancel_account_setup_form(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    first_run: bool,
    app_cx: &mut App,
) {
    if first_run {
        let _ = modal_entity.update(app_cx, |modal, cx| {
            modal.show_mode(AccountSetupMode::Choice, cx);
        });
    } else {
        close_account_setup_modal(state_entity, app_cx);
    }
}

fn start_account_setup_oauth(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    provider: AccountSetupProvider,
    app_cx: &mut App,
) {
    if modal_entity.read(app_cx).is_busy() {
        return;
    }

    let base_url = state_entity.read(app_cx).api_base_url.clone();
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.oauth_loading = Some(provider);
        modal.status = None;
        cx.notify();
    });
    state_entity.update(app_cx, |state, cx| {
        state.last_error = None;
        cx.notify();
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { authorize_account_setup(base_url, provider) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let status = match &result {
                    Ok(_) => (
                        format!("Browser opened for {} authorization.", provider.label()),
                        true,
                    ),
                    Err(err) => (err.to_string(), false),
                };

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| {
                    modal.set_status(Some(status.clone()), cx);
                }) {
                    tracing::warn!("failed to apply native OAuth status: {err}");
                }

                if !status.1 {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(status.0);
                        cx.notify();
                    }) {
                        tracing::warn!("failed to apply native OAuth error: {err}");
                    }
                }
            }
        })
        .detach();
}

fn start_account_setup_create(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    app_cx: &mut App,
) {
    let (mode, draft, busy) = {
        let modal = modal_entity.read(app_cx);
        (
            modal.mode,
            AccountSetupFormDraft {
                name: modal.name_input.read(app_cx).text().to_string(),
                email: modal.email_input.read(app_cx).text().to_string(),
                imap_host: modal.imap_host_input.read(app_cx).text().to_string(),
                imap_port: modal.imap_port_input.read(app_cx).text().to_string(),
                imap_password: modal.imap_password_input.read(app_cx).text().to_string(),
                smtp_host: modal.smtp_host_input.read(app_cx).text().to_string(),
                smtp_port: modal.smtp_port_input.read(app_cx).text().to_string(),
                smtp_password: modal.smtp_password_input.read(app_cx).text().to_string(),
                same_password: modal.same_password,
            },
            modal.is_busy(),
        )
    };
    if busy {
        return;
    }

    let request = match mode {
        AccountSetupMode::ManualImap => build_manual_account_request(draft),
        AccountSetupMode::MicrosoftAppPassword => build_microsoft_app_password_request(draft),
        AccountSetupMode::Choice => Err("Choose an account setup method.".to_string()),
    };
    let request = match request {
        Ok(request) => request,
        Err(message) => {
            let _ = modal_entity.update(app_cx, |modal, cx| {
                modal.set_status(Some((message.clone(), false)), cx);
            });
            state_entity.update(app_cx, |state, cx| {
                state.last_error = Some(message);
                cx.notify();
            });
            return;
        }
    };

    let base_url = state_entity.read(app_cx).api_base_url.clone();
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.submitting = true;
        modal.status = None;
        cx.notify();
    });
    state_entity.update(app_cx, |state, cx| {
        state.last_error = None;
        cx.notify();
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { create_account_and_reload(base_url, request) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_account_create_reload(state, loaded);
                        }
                        Err(err) => {
                            state.last_error = Some(err.to_string());
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native account create reload: {err}");
                }

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| {
                    if let Some(message) = error_message {
                        modal.set_status(Some((message, false)), cx);
                    } else {
                        modal.reset(cx);
                    }
                }) {
                    tracing::warn!("failed to apply native account create status: {err}");
                }
            }
        })
        .detach();
}

fn start_account_setup_refresh(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AccountSetupModal>,
    app_cx: &mut App,
) {
    if modal_entity.read(app_cx).is_busy() {
        return;
    }

    let base_url = state_entity.read(app_cx).api_base_url.clone();
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.refreshing = true;
        modal.status = None;
        cx.notify();
    });
    state_entity.update(app_cx, |state, cx| {
        state.last_error = None;
        cx.notify();
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { reload_accounts(base_url) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());
                let loaded_has_accounts = result
                    .as_ref()
                    .map(|loaded| !loaded.accounts.is_empty())
                    .unwrap_or(false);

                if loaded_has_accounts {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        if let Ok(loaded) = result {
                            apply_account_create_reload(state, loaded);
                            sync_window_title(state, cx);
                            cx.notify();
                        }
                    }) {
                        tracing::warn!("failed to apply native account refresh: {err}");
                    }
                }

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| {
                    if loaded_has_accounts {
                        modal.reset(cx);
                    } else if let Some(message) = error_message.clone() {
                        modal.set_status(Some((message.clone(), false)), cx);
                    } else {
                        modal.set_status(
                            Some(("No authorized account found yet.".to_string(), false)),
                            cx,
                        );
                    }
                }) {
                    tracing::warn!("failed to apply native account refresh status: {err}");
                }

                if let Some(message) = error_message {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(message);
                        cx.notify();
                    }) {
                        tracing::warn!("failed to apply native account refresh error: {err}");
                    }
                }
            }
        })
        .detach();
}

fn open_settings_modal(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    app_cx: &mut App,
) {
    state_entity.update(app_cx, |state, cx| {
        state.modals.settings_open = true;
        state.modals.advanced_search_open = false;
        cx.notify();
    });
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.reset(cx);
    });
}

fn close_settings_modal(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        state.modals.settings_open = false;
        cx.notify();
    });
}

fn handle_settings_modal_key(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    key: &str,
    command: bool,
    app_cx: &mut App,
) -> bool {
    match key {
        "escape" => {
            let editor_open = modal_entity.read(app_cx).rule_editor_mode.is_some();
            if editor_open {
                cancel_settings_rule_editor(modal_entity, app_cx);
            } else {
                close_settings_modal(state_entity, app_cx);
            }
            true
        }
        "enter" | "return" => {
            let active = {
                let modal = modal_entity.read(app_cx);
                (modal.active_tab.clone(), modal.rule_editor_mode.is_some())
            };
            match active {
                (SettingsTab::Rules(_), true) => {
                    start_settings_rule_save(state_entity, modal_entity, app_cx);
                    true
                }
                (SettingsTab::Webhooks, _) => {
                    start_settings_webhook_create(state_entity, modal_entity, app_cx);
                    true
                }
                (SettingsTab::Account(account_id), _) if command => {
                    start_settings_save(state_entity, modal_entity, account_id, app_cx);
                    true
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn select_settings_account(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    app_cx: &mut App,
) {
    let selection = {
        let state = state_entity.read(app_cx);
        state
            .accounts
            .iter()
            .find(|account| account.id == account_id)
            .cloned()
            .map(|account| (state.api_base_url.clone(), account))
    };

    let Some((base_url, account)) = selection else {
        let message = "Account not found.".to_string();
        state_entity.update(app_cx, |state, cx| {
            state.last_error = Some(message.clone());
            cx.notify();
        });
        let _ = modal_entity.update(app_cx, |modal, cx| {
            modal.set_status(Some((message, false)), cx);
        });
        return;
    };

    let account_id = account.id.clone();
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.select_account(&account, cx);
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { load_account_signature(base_url, account_id.clone()) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    let requested_account_id = account.id.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| match result {
                    Ok(settings) => modal.apply_account_settings(settings, cx),
                    Err(err) => {
                        modal.fail_account_settings(&requested_account_id, err.to_string(), cx);
                    }
                }) {
                    tracing::warn!("failed to apply native account settings load: {err}");
                }

                if let Some(message) = error_message {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(message);
                        cx.notify();
                    }) {
                        tracing::warn!("failed to apply native account settings error: {err}");
                    }
                }
            }
        })
        .detach();
}

fn select_settings_webhooks(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    app_cx: &mut App,
) {
    let base_url = state_entity.read(app_cx).api_base_url.clone();
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.select_webhooks(cx);
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { load_settings_webhooks(base_url) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| match result {
                    Ok(webhooks) => modal.apply_webhooks(webhooks, None, false, cx),
                    Err(err) => modal.fail_webhooks(err.to_string(), cx),
                }) {
                    tracing::warn!("failed to apply native webhooks load: {err}");
                }

                if let Some(message) = error_message {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(message);
                        cx.notify();
                    }) {
                        tracing::warn!("failed to apply native webhooks error: {err}");
                    }
                }
            }
        })
        .detach();
}

fn select_settings_rules(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    app_cx: &mut App,
) {
    let base_url = state_entity.read(app_cx).api_base_url.clone();
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.select_rules(account_id.clone(), cx);
    });

    let requested_account_id = account_id.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { load_settings_rules(base_url, account_id.clone()) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| match result {
                    Ok(rules) => modal.apply_rules(&requested_account_id, rules, None, cx),
                    Err(err) => {
                        modal.fail_rules(&requested_account_id, err.to_string(), cx);
                    }
                }) {
                    tracing::warn!("failed to apply native rules load: {err}");
                }

                if let Some(message) = error_message {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(message);
                        cx.notify();
                    }) {
                        tracing::warn!("failed to apply native rules error: {err}");
                    }
                }
            }
        })
        .detach();
}

fn confirm_settings_account_delete(
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    app_cx: &mut App,
) {
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.confirm_delete_account = Some(account_id);
        modal.status = None;
        cx.notify();
    });
}

fn cancel_settings_account_delete(modal_entity: &Entity<SettingsModal>, app_cx: &mut App) {
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.confirm_delete_account = None;
        modal.status = None;
        cx.notify();
    });
}

fn start_settings_save(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    app_cx: &mut App,
) {
    let (name, sync_days, signature) = {
        let modal = modal_entity.read(app_cx);
        (
            modal.name_input.read(app_cx).text().to_string(),
            modal.sync_days_input.read(app_cx).text().to_string(),
            modal.signature_input.read(app_cx).text().to_string(),
        )
    };

    let request = match build_account_settings_update_request(name, sync_days, signature) {
        Ok(request) => request,
        Err(message) => {
            let _ = modal_entity.update(app_cx, |modal, cx| {
                modal.set_status(Some((message.clone(), false)), cx);
            });
            state_entity.update(app_cx, |state, cx| {
                state.last_error = Some(message);
                cx.notify();
            });
            return;
        }
    };

    let (base_url, refresh_request) = state_entity.update(app_cx, |state, cx| {
        let refresh_request = begin_sync_complete_refresh(state);
        state.last_error = None;
        cx.notify();
        (state.api_base_url.clone(), refresh_request)
    });
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.saving_account = true;
        modal.status = None;
        cx.notify();
    });

    let load_request = refresh_request.clone();
    let requested_account_id = account_id.clone();
    let task = app_cx.background_executor().spawn(async move {
        update_account_settings_and_refresh(base_url, account_id.clone(), request, load_request)
    });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let modal_status = match &result {
                    Ok(_) => ("Saved".to_string(), true),
                    Err(err) => (err.to_string(), false),
                };

                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_loaded_sync_refresh(state, refresh_request, loaded);
                        }
                        Err(err) => {
                            reject_sync_refresh(state, refresh_request, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native settings save refresh: {err}");
                }

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| {
                    if modal.active_tab == SettingsTab::Account(requested_account_id.clone()) {
                        modal.set_status(Some(modal_status), cx);
                    } else {
                        modal.saving_account = false;
                        cx.notify();
                    }
                }) {
                    tracing::warn!("failed to apply native settings save status: {err}");
                }
            }
        })
        .detach();
}

fn start_settings_account_delete(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    app_cx: &mut App,
) {
    let Some(base_url) = ({
        let state = state_entity.read(app_cx);
        state
            .accounts
            .iter()
            .any(|account| account.id == account_id)
            .then(|| state.api_base_url.clone())
    }) else {
        let message = "Account not found.".to_string();
        let _ = modal_entity.update(app_cx, |modal, cx| {
            modal.set_status(Some((message.clone(), false)), cx);
        });
        state_entity.update(app_cx, |state, cx| {
            state.last_error = Some(message);
            cx.notify();
        });
        return;
    };

    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.deleting_account = true;
        modal.status = None;
        cx.notify();
    });
    state_entity.update(app_cx, |state, cx| {
        state.last_error = None;
        cx.notify();
    });

    let requested_account_id = account_id.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { delete_account_and_reload(base_url, account_id) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_account_delete_reload(state, &requested_account_id, loaded);
                        }
                        Err(err) => {
                            state.last_error = Some(err.to_string());
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native account delete reload: {err}");
                }

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| {
                    if let Some(message) = error_message {
                        modal.set_status(Some((message, false)), cx);
                    } else {
                        modal.reset(cx);
                    }
                }) {
                    tracing::warn!("failed to apply native account delete status: {err}");
                }
            }
        })
        .detach();
}

fn authorize_account_setup(base_url: String, provider: AccountSetupProvider) -> Result<String> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        let response = match provider {
            AccountSetupProvider::Gmail => client.gmail_authorize().await?,
            AccountSetupProvider::Microsoft => client.microsoft_authorize().await?,
        };
        open_url_in_browser(&response.url)
            .with_context(|| format!("open {} authorization URL", provider.label()))?;
        Ok(response.url)
    })
}

fn open_url_in_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .context("launch default browser")?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .context("launch default browser")?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .context("launch default browser")?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err(anyhow!(
        "opening a browser is not supported on this platform"
    ))
}

fn create_account_and_reload(base_url: String, request: CreateAccount) -> Result<NativeAppState> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client.create_account(&request).await.with_context(|| {
            format!(
                "create {} account for {}",
                request.provider_type, request.email
            )
        })?;
        let mut loaded = load_accounts(&client)
            .await
            .context("reload accounts after creating account")?;
        loaded.api_base_url = base_url;
        Ok(loaded)
    })
}

fn reload_accounts(base_url: String) -> Result<NativeAppState> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        let mut loaded = load_accounts(&client).await.context("reload accounts")?;
        loaded.api_base_url = base_url;
        Ok(loaded)
    })
}

fn start_settings_rule_toggle(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    rule_id: String,
    app_cx: &mut App,
) {
    start_settings_rule_mutation(
        state_entity,
        modal_entity,
        account_id,
        SettingsRuleMutation::Toggle {
            rule_id: rule_id.clone(),
        },
        rule_id,
        "Rule updated",
        app_cx,
    );
}

fn start_settings_rule_delete(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    rule_id: String,
    app_cx: &mut App,
) {
    start_settings_rule_mutation(
        state_entity,
        modal_entity,
        account_id,
        SettingsRuleMutation::Delete {
            rule_id: rule_id.clone(),
        },
        rule_id,
        "Rule deleted",
        app_cx,
    );
}

fn start_settings_rule_new(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    app_cx: &mut App,
) {
    let folders = state_entity
        .read(app_cx)
        .folders_by_account
        .get(&account_id)
        .cloned()
        .unwrap_or_default();
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.start_new_rule_editor(account_id, folders, cx);
    });
}

fn start_settings_rule_edit(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    rule: RuleDto,
    app_cx: &mut App,
) {
    let folders = state_entity
        .read(app_cx)
        .folders_by_account
        .get(&account_id)
        .cloned()
        .unwrap_or_default();
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.start_edit_rule_editor(account_id, rule, folders, cx);
    });
}

fn cancel_settings_rule_editor(modal_entity: &Entity<SettingsModal>, app_cx: &mut App) {
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.reset_rule_editor(cx);
        modal.status = None;
        cx.notify();
    });
}

fn start_settings_rule_save(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    app_cx: &mut App,
) {
    let (saving, mode, account_id, name_input, match_mode, conditions, actions) = {
        let modal = modal_entity.read(app_cx);
        (
            modal.rule_saving,
            modal.rule_editor_mode.clone(),
            modal.rule_editor_account_id.clone(),
            modal.rule_name_input.clone(),
            modal.rule_match_mode.clone(),
            modal
                .rule_condition_rows
                .iter()
                .map(|row| row.condition.clone())
                .collect::<Vec<_>>(),
            modal
                .rule_action_rows
                .iter()
                .map(|row| SettingsRuleActionDraft {
                    action_type: row.action_type.clone(),
                    action_value: row.action_value.clone(),
                })
                .collect::<Vec<_>>(),
        )
    };
    if saving {
        return;
    }
    let name = name_input.read(app_cx).text().to_string();
    let request = match build_settings_rule_save_request(
        mode, account_id, name, match_mode, conditions, actions,
    ) {
        Ok(request) => request,
        Err(message) => {
            let _ = modal_entity.update(app_cx, |modal, cx| {
                modal.status = Some((message.clone(), false));
                modal.rule_saving = false;
                cx.notify();
            });
            state_entity.update(app_cx, |state, cx| {
                state.last_error = Some(message);
                cx.notify();
            });
            return;
        }
    };

    let base_url = state_entity.read(app_cx).api_base_url.clone();
    let account_id = request.account_id.clone();
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.rule_saving = true;
        modal.status = None;
        cx.notify();
    });
    state_entity.update(app_cx, |state, cx| {
        state.last_error = None;
        cx.notify();
    });

    let requested_account_id = account_id.clone();
    let task = app_cx.background_executor().spawn(async move {
        mutate_settings_rules(base_url, account_id, SettingsRuleMutation::Save(request))
    });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| match result {
                    Ok(rules) => {
                        modal.apply_rules(
                            &requested_account_id,
                            rules,
                            Some(("Rule saved".to_string(), true)),
                            cx,
                        );
                        modal.reset_rule_editor(cx);
                    }
                    Err(err) => {
                        modal.fail_rules(&requested_account_id, err.to_string(), cx);
                    }
                }) {
                    tracing::warn!("failed to apply native rule save: {err}");
                }

                if let Some(message) = error_message {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(message);
                        cx.notify();
                    }) {
                        tracing::warn!("failed to apply native rule save error: {err}");
                    }
                }
            }
        })
        .detach();
}

fn start_settings_rule_mutation(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    account_id: String,
    mutation: SettingsRuleMutation,
    busy_id: String,
    success_message: &'static str,
    app_cx: &mut App,
) {
    let base_url = state_entity.read(app_cx).api_base_url.clone();
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.rule_busy_ids.insert(busy_id);
        modal.status = None;
        cx.notify();
    });
    state_entity.update(app_cx, |state, cx| {
        state.last_error = None;
        cx.notify();
    });

    let requested_account_id = account_id.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { mutate_settings_rules(base_url, account_id, mutation) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| match result {
                    Ok(rules) => modal.apply_rules(
                        &requested_account_id,
                        rules,
                        Some((success_message.to_string(), true)),
                        cx,
                    ),
                    Err(err) => {
                        modal.fail_rules(&requested_account_id, err.to_string(), cx);
                    }
                }) {
                    tracing::warn!("failed to apply native rule mutation: {err}");
                }

                if let Some(message) = error_message {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(message);
                        cx.notify();
                    }) {
                        tracing::warn!("failed to apply native rule mutation error: {err}");
                    }
                }
            }
        })
        .detach();
}

fn start_settings_webhook_create(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    app_cx: &mut App,
) {
    let (url, secret, events) = {
        let modal = modal_entity.read(app_cx);
        (
            modal.webhook_url_input.read(app_cx).text().to_string(),
            modal.webhook_secret_input.read(app_cx).text().to_string(),
            modal.webhook_events.clone(),
        )
    };
    let request = match build_create_webhook_request(url, secret, events) {
        Ok(request) => request,
        Err(message) => {
            let _ = modal_entity.update(app_cx, |modal, cx| {
                modal.fail_webhooks(message.clone(), cx);
            });
            state_entity.update(app_cx, |state, cx| {
                state.last_error = Some(message);
                cx.notify();
            });
            return;
        }
    };

    start_settings_webhook_mutation(
        state_entity,
        modal_entity,
        SettingsWebhookMutation::Create(request),
        None,
        "Webhook added",
        true,
        app_cx,
    );
}

fn start_settings_webhook_toggle(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    webhook_id: String,
    enabled: bool,
    app_cx: &mut App,
) {
    start_settings_webhook_mutation(
        state_entity,
        modal_entity,
        SettingsWebhookMutation::Toggle {
            webhook_id: webhook_id.clone(),
            enabled,
        },
        Some(webhook_id),
        "Webhook updated",
        false,
        app_cx,
    );
}

fn start_settings_webhook_delete(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    webhook_id: String,
    app_cx: &mut App,
) {
    start_settings_webhook_mutation(
        state_entity,
        modal_entity,
        SettingsWebhookMutation::Delete {
            webhook_id: webhook_id.clone(),
        },
        Some(webhook_id),
        "Webhook deleted",
        false,
        app_cx,
    );
}

fn start_settings_webhook_mutation(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    mutation: SettingsWebhookMutation,
    busy_id: Option<String>,
    success_message: &'static str,
    clear_new: bool,
    app_cx: &mut App,
) {
    let base_url = state_entity.read(app_cx).api_base_url.clone();
    let _ = modal_entity.update(app_cx, |modal, cx| {
        if let Some(id) = busy_id.as_ref() {
            modal.webhook_busy_ids.insert(id.clone());
        } else {
            modal.adding_webhook = true;
        }
        modal.status = None;
        cx.notify();
    });
    state_entity.update(app_cx, |state, cx| {
        state.last_error = None;
        cx.notify();
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { mutate_settings_webhooks(base_url, mutation) });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let error_message = result.as_ref().err().map(|err| err.to_string());

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| match result {
                    Ok(webhooks) => modal.apply_webhooks(
                        webhooks,
                        Some((success_message.to_string(), true)),
                        clear_new,
                        cx,
                    ),
                    Err(err) => modal.fail_webhooks(err.to_string(), cx),
                }) {
                    tracing::warn!("failed to apply native webhook mutation: {err}");
                }

                if let Some(message) = error_message {
                    if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                        state.last_error = Some(message);
                        cx.notify();
                    }) {
                        tracing::warn!("failed to apply native webhook mutation error: {err}");
                    }
                }
            }
        })
        .detach();
}

fn start_folder_exclusion_toggle(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<SettingsModal>,
    folder_id: String,
    next_excluded: bool,
    app_cx: &mut App,
) {
    let (base_url, refresh_request) = state_entity.update(app_cx, |state, cx| {
        let refresh_request = begin_sync_complete_refresh(state);
        state.last_error = None;
        cx.notify();
        (state.api_base_url.clone(), refresh_request)
    });
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.status = Some(("Updating folder...".to_string(), true));
        cx.notify();
    });

    let load_request = refresh_request.clone();
    let task = app_cx.background_executor().spawn(async move {
        patch_folder_exclusion_and_refresh(base_url, folder_id, next_excluded, load_request)
    });
    let state_entity = state_entity.clone();
    let modal_entity = modal_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let modal_status = match &result {
                    Ok(_) => ("Folder updated".to_string(), true),
                    Err(err) => (err.to_string(), false),
                };

                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_loaded_sync_refresh(state, refresh_request, loaded);
                        }
                        Err(err) => {
                            reject_sync_refresh(state, refresh_request, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native folder exclusion refresh: {err}");
                }

                if let Err(err) = modal_entity.update(&mut async_cx, |modal, cx| {
                    modal.set_status(Some(modal_status), cx);
                }) {
                    tracing::warn!("failed to apply native folder exclusion status: {err}");
                }
            }
        })
        .detach();
}

fn delete_account_and_reload(base_url: String, account_id: String) -> Result<NativeAppState> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .delete_account(&account_id)
            .await
            .with_context(|| format!("delete account {account_id}"))?;
        let mut loaded = load_accounts(&client)
            .await
            .with_context(|| "reload accounts after deleting account")?;
        loaded.api_base_url = base_url;
        Ok(loaded)
    })
}

fn load_settings_webhooks(base_url: String) -> Result<Vec<Webhook>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client.list_webhooks().await.context("list webhooks")
    })
}

fn load_settings_rules(base_url: String, account_id: String) -> Result<Vec<RuleDto>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .list_rules(&account_id)
            .await
            .with_context(|| format!("list rules for {account_id}"))
    })
}

fn mutate_settings_rules(
    base_url: String,
    account_id: String,
    mutation: SettingsRuleMutation,
) -> Result<Vec<RuleDto>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        match mutation {
            SettingsRuleMutation::Save(request) => match request.mode {
                SettingsRuleEditorMode::New => {
                    client
                        .create_rule(
                            &request.account_id,
                            &request.name,
                            Some(&request.match_mode),
                            Some(request.conditions),
                            Some(request.actions),
                        )
                        .await
                        .with_context(|| format!("create rule for {}", request.account_id))?;
                }
                SettingsRuleEditorMode::Edit(rule_id) => {
                    client
                        .update_rule(
                            &rule_id,
                            &UpdateRuleBody {
                                name: Some(request.name),
                                match_mode: Some(request.match_mode),
                                conditions: Some(request.conditions),
                                actions: Some(request.actions),
                            },
                        )
                        .await
                        .with_context(|| format!("update rule {rule_id}"))?;
                }
            },
            SettingsRuleMutation::Toggle { rule_id } => {
                client
                    .toggle_rule(&rule_id)
                    .await
                    .with_context(|| format!("toggle rule {rule_id}"))?;
            }
            SettingsRuleMutation::Delete { rule_id } => {
                client
                    .delete_rule(&rule_id)
                    .await
                    .with_context(|| format!("delete rule {rule_id}"))?;
            }
        }
        client
            .list_rules(&account_id)
            .await
            .with_context(|| format!("reload rules for {account_id}"))
    })
}

fn mutate_settings_webhooks(
    base_url: String,
    mutation: SettingsWebhookMutation,
) -> Result<Vec<Webhook>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        match mutation {
            SettingsWebhookMutation::Create(request) => {
                client
                    .create_webhook(&request)
                    .await
                    .context("create webhook")?;
            }
            SettingsWebhookMutation::Toggle {
                webhook_id,
                enabled,
            } => {
                client
                    .update_webhook(
                        &webhook_id,
                        &UpdateWebhook {
                            url: None,
                            secret: None,
                            events: None,
                            enabled: Some(enabled),
                        },
                    )
                    .await
                    .with_context(|| format!("update webhook {webhook_id}"))?;
            }
            SettingsWebhookMutation::Delete { webhook_id } => {
                client
                    .delete_webhook(&webhook_id)
                    .await
                    .with_context(|| format!("delete webhook {webhook_id}"))?;
            }
        }
        client.list_webhooks().await.context("reload webhooks")
    })
}

fn apply_account_create_reload(state: &mut NativeAppState, mut loaded: NativeAppState) {
    let had_accounts = !state.accounts.is_empty();
    let theme = state.theme.clone();
    let density = state.density.clone();
    let selected_view = state.selected_view.clone();
    let selected_folder = state.selected_folder.clone();
    let selected_label = state.selected_label.clone();
    let selected_message = state.selected_message.clone();
    let selected_message_ids = state.selected_message_ids.clone();
    let selected_full_message = state.selected_full_message.clone();
    let selected_message_resource = state.selected_message_resource.clone();
    let selected_message_labels = state.selected_message_labels.clone();
    let selected_message_labels_resource = state.selected_message_labels_resource.clone();
    let selected_event = state.selected_event.clone();
    let search = state.search.clone();
    let messages = state.messages.clone();
    let messages_resource = state.messages_resource.clone();
    let palette = state.palette.clone();
    let sync = state.sync.clone();
    let compose = state.compose.clone();

    loaded.theme = theme;
    loaded.density = density;
    if had_accounts {
        loaded.selected_view = selected_view;
        loaded.selected_folder = selected_folder;
        loaded.selected_label = selected_label;
        loaded.selected_message = selected_message;
        loaded.selected_message_ids = selected_message_ids;
        loaded.selected_full_message = selected_full_message;
        loaded.selected_message_resource = selected_message_resource;
        loaded.selected_message_labels = selected_message_labels;
        loaded.selected_message_labels_resource = selected_message_labels_resource;
        loaded.selected_event = selected_event;
        loaded.search = search;
        loaded.messages = messages;
        loaded.messages_resource = messages_resource;
    } else {
        loaded.selected_view = SelectedView::Mail;
    }
    loaded.palette = palette;
    loaded.sync = sync;
    loaded.compose = compose;
    loaded.modals = ModalStates {
        account_setup_open: false,
        settings_open: false,
        advanced_search_open: false,
    };
    loaded.last_error = None;
    *state = loaded;
}

fn apply_account_delete_reload(
    state: &mut NativeAppState,
    deleted_account_id: &str,
    mut loaded: NativeAppState,
) {
    let theme = state.theme.clone();
    let density = state.density.clone();
    let selected_view = state.selected_view.clone();
    let selected_event = state.selected_event.clone();
    let palette = state.palette.clone();
    let sync = state.sync.clone();
    let account_setup_open = state.modals.account_setup_open;
    let compose = if state.compose.account_id.as_deref() == Some(deleted_account_id) {
        ComposeState {
            revision: state.compose.revision.saturating_add(1),
            ..ComposeState::default()
        }
    } else {
        state.compose.clone()
    };

    loaded.theme = theme;
    loaded.density = density;
    loaded.selected_view = selected_view;
    loaded.selected_event = selected_event;
    loaded.palette = palette;
    loaded.sync = sync;
    loaded.compose = compose;
    loaded.modals = ModalStates {
        account_setup_open,
        settings_open: false,
        advanced_search_open: false,
    };
    loaded.last_error = None;
    *state = loaded;
}

fn build_manual_account_request(
    draft: AccountSetupFormDraft,
) -> std::result::Result<CreateAccount, String> {
    let name = validate_account_setup_name(&draft.name)?;
    let email = validate_account_setup_email(&draft.email)?;
    let imap_host = validate_account_setup_host(&draft.imap_host, "IMAP host")?;
    let imap_port = parse_account_setup_port(&draft.imap_port, "IMAP")?;
    let smtp_host = validate_account_setup_host(&draft.smtp_host, "SMTP host")?;
    let smtp_port = parse_account_setup_port(&draft.smtp_port, "SMTP")?;
    let imap_password = validate_account_setup_password(&draft.imap_password, "IMAP password")?;
    let smtp_password = if draft.same_password {
        imap_password.clone()
    } else {
        validate_account_setup_password(&draft.smtp_password, "SMTP password")?
    };

    Ok(CreateAccount {
        name,
        email,
        provider_type: "imap".to_string(),
        auth_type: "basic".to_string(),
        oauth_token_json: None,
        host: Some(imap_host),
        port: Some(imap_port),
        use_ssl: None,
        password: Some(imap_password),
        smtp_host: Some(smtp_host),
        smtp_port: Some(smtp_port),
        smtp_password: Some(smtp_password),
    })
}

fn build_microsoft_app_password_request(
    draft: AccountSetupFormDraft,
) -> std::result::Result<CreateAccount, String> {
    let name = validate_account_setup_name(&draft.name)?;
    let email = validate_account_setup_email(&draft.email)?;
    let imap_host = validate_account_setup_host(&draft.imap_host, "IMAP host")?;
    let imap_port = parse_account_setup_port(&draft.imap_port, "IMAP")?;
    let smtp_host = validate_account_setup_host(&draft.smtp_host, "SMTP host")?;
    let smtp_port = parse_account_setup_port(&draft.smtp_port, "SMTP")?;
    let password = validate_account_setup_password(&draft.imap_password, "App password")?;

    Ok(CreateAccount {
        name,
        email,
        provider_type: "microsoft365".to_string(),
        auth_type: "app_password".to_string(),
        oauth_token_json: None,
        host: Some(imap_host),
        port: Some(imap_port),
        use_ssl: None,
        password: Some(password.clone()),
        smtp_host: Some(smtp_host),
        smtp_port: Some(smtp_port),
        smtp_password: Some(password),
    })
}

fn validate_account_setup_name(value: &str) -> std::result::Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err("Enter a display name.".to_string())
    } else {
        Ok(value)
    }
}

fn validate_account_setup_email(value: &str) -> std::result::Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err("Enter an email address.".to_string());
    }
    if !value.contains('@') {
        return Err("Enter a valid email address.".to_string());
    }
    Ok(value)
}

fn validate_account_setup_host(
    value: &str,
    label: &'static str,
) -> std::result::Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(format!("Enter a {label}."))
    } else {
        Ok(value)
    }
}

fn validate_account_setup_password(
    value: &str,
    label: &'static str,
) -> std::result::Result<String, String> {
    if value.trim().is_empty() {
        Err(format!("Enter a {label}."))
    } else {
        Ok(value.to_string())
    }
}

fn parse_account_setup_port(value: &str, label: &'static str) -> std::result::Result<i64, String> {
    let port = value
        .trim()
        .parse::<i64>()
        .map_err(|_| format!("{label} port must be a positive whole number."))?;
    if !(1..=65535).contains(&port) {
        return Err(format!("{label} port must be between 1 and 65535."));
    }
    Ok(port)
}

fn build_create_webhook_request(
    url: String,
    secret: String,
    events: String,
) -> std::result::Result<CreateWebhook, String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("Enter a webhook URL.".to_string());
    }

    let events = match events.trim() {
        "" | "new_message" => "new_message".to_string(),
        "all" => "all".to_string(),
        _ => return Err("Choose a supported webhook event set.".to_string()),
    };
    let secret = if secret.trim().is_empty() {
        None
    } else {
        Some(secret.trim().to_string())
    };

    Ok(CreateWebhook {
        url,
        secret,
        events: Some(events),
        account_id: None,
    })
}

fn build_settings_rule_save_request(
    mode: Option<SettingsRuleEditorMode>,
    account_id: Option<String>,
    name: String,
    match_mode: MatchMode,
    conditions: Vec<Condition>,
    actions: Vec<SettingsRuleActionDraft>,
) -> std::result::Result<SettingsRuleSaveRequest, String> {
    let mode = mode.ok_or_else(|| "Open a rule editor first.".to_string())?;
    let account_id = account_id
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "Rule account not found.".to_string())?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Enter a rule name.".to_string());
    }
    if actions.is_empty() {
        return Err("Add at least one rule action.".to_string());
    }

    let conditions = conditions
        .into_iter()
        .map(|condition| {
            let value = if condition_field_type(&condition.field) == ConditionFieldType::Boolean {
                if condition.value.trim() == "false" {
                    "false".to_string()
                } else {
                    "true".to_string()
                }
            } else {
                condition.value.trim().to_string()
            };
            ConditionInput {
                field: condition_field_value(&condition.field).to_string(),
                operator: condition_operator_value(&condition.operator).to_string(),
                value,
            }
        })
        .collect::<Vec<_>>();

    let mut request_actions = Vec::with_capacity(actions.len());
    for action in actions {
        let action_type = action.action_type.trim().to_string();
        if !is_supported_rule_action(&action_type) {
            return Err("Choose a supported rule action.".to_string());
        }

        let action_value = if rule_action_needs_folder(&action_type) {
            let value = action.action_value.unwrap_or_default().trim().to_string();
            if value.is_empty() {
                return Err("Choose a folder for move actions.".to_string());
            }
            Some(value)
        } else {
            None
        };

        request_actions.push(ActionInput {
            action_type,
            action_value,
        });
    }

    Ok(SettingsRuleSaveRequest {
        mode,
        account_id,
        name,
        match_mode: match_mode_value(&match_mode).to_string(),
        conditions,
        actions: request_actions,
    })
}

fn match_mode_value(match_mode: &MatchMode) -> &'static str {
    match match_mode {
        MatchMode::All => "all",
        MatchMode::Any => "any",
    }
}

fn is_supported_rule_action(value: &str) -> bool {
    rule_action_label(value).is_some()
}

fn rule_action_needs_folder(value: &str) -> bool {
    value == "move_to_folder"
}

fn rule_action_label(value: &str) -> Option<&'static str> {
    match value {
        "mark_read" => Some("Mark as read"),
        "mark_unread" => Some("Mark as unread"),
        "flag" => Some("Flag message"),
        "unflag" => Some("Unflag message"),
        "archive" => Some("Archive"),
        "delete" => Some("Delete"),
        "move_to_folder" => Some("Move to folder"),
        _ => None,
    }
}

fn rule_folder_option_label(folder: &Folder) -> String {
    if folder.full_path.trim().is_empty() {
        folder.name.clone()
    } else {
        folder.full_path.clone()
    }
}

fn rule_counts_text(rule: &RuleDto) -> String {
    format!(
        "{} condition{}, {} action{}",
        rule.conditions.len(),
        if rule.conditions.len() == 1 { "" } else { "s" },
        rule.actions.len(),
        if rule.actions.len() == 1 { "" } else { "s" }
    )
}

fn update_account_settings_and_refresh(
    base_url: String,
    account_id: String,
    request: UpdateAccount,
    refresh_request: NativeSyncRefreshRequest,
) -> Result<LoadedSyncRefresh> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .update_account(&account_id, &request)
            .await
            .with_context(|| format!("update account settings for {account_id}"))?;
        load_sync_refresh_with_client(&client, refresh_request).await
    })
}

fn patch_folder_exclusion_and_refresh(
    base_url: String,
    folder_id: String,
    next_excluded: bool,
    refresh_request: NativeSyncRefreshRequest,
) -> Result<LoadedSyncRefresh> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .patch_folder(
                &folder_id,
                &PatchFolder {
                    is_excluded: Some(next_excluded),
                },
            )
            .await
            .with_context(|| format!("update folder exclusion for {folder_id}"))?;
        load_sync_refresh_with_client(&client, refresh_request).await
    })
}

async fn load_sync_refresh_with_client(
    client: &EmailClient,
    request: NativeSyncRefreshRequest,
) -> Result<LoadedSyncRefresh> {
    let tree = load_account_tree(client).await?;
    let messages = if let Some(source) = request.source {
        Some(load_message_collection_with_client(client, source, request.selected_message).await?)
    } else {
        None
    };

    Ok(LoadedSyncRefresh {
        accounts: tree.accounts,
        folders_by_account: tree.folders_by_account,
        labels: tree.labels,
        messages,
    })
}

fn build_account_settings_update_request(
    name: String,
    sync_days: String,
    signature: String,
) -> std::result::Result<UpdateAccount, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Enter a display name.".to_string());
    }

    let sync_days_limit = if sync_days.trim().is_empty() {
        None
    } else {
        let parsed = sync_days
            .trim()
            .parse::<i64>()
            .map_err(|_| "Download days must be a positive whole number.".to_string())?;
        if parsed < 1 {
            return Err("Download days must be a positive whole number.".to_string());
        }
        Some(parsed)
    };

    let signature = if signature.trim().is_empty() {
        None
    } else {
        Some(signature)
    };

    Ok(UpdateAccount {
        name: Some(name),
        sync_days_limit: Some(sync_days_limit),
        signature: Some(signature),
    })
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

fn sync_update_requests_refresh(update: &NativeSyncUpdate) -> bool {
    matches!(
        update,
        NativeSyncUpdate::Event(SyncEvent::SyncComplete { .. })
    )
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
    Search(String),
    AdvancedSearch(ConditionGroup),
}

impl NativeMessageSource {
    fn from_state(state: &NativeAppState) -> Option<Self> {
        if let Some(group) = state.search.condition_group.as_ref() {
            return Some(Self::AdvancedSearch(group.clone()));
        }

        if let Some(query) = state.search.submitted_query.as_ref() {
            return Some(Self::Search(query.clone()));
        }

        Self::route_from_state(state)
    }

    fn route_from_state(state: &NativeAppState) -> Option<Self> {
        if let Some(label_id) = state.selected_label.as_ref() {
            return Some(Self::Label(label_id.clone()));
        }

        let selected_folder = state.selected_folder.as_ref()?;
        if let Some(kind) = selected_folder.strip_prefix("smart:") {
            Some(Self::SmartFolder(kind.to_string()))
        } else {
            Some(Self::Folder(selected_folder.clone()))
        }
    }

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
            Self::Search(query) => {
                if state.search.submitted_query.is_none() {
                    state.search.previous_source = Self::route_from_state(state);
                }
                state.search.query = query.clone();
                state.search.submitted_query = Some(query.clone());
                state.search.condition_group = None;
                state.selected_folder = None;
                state.selected_label = None;
            }
            Self::AdvancedSearch(group) => {
                if state.search.condition_group.is_none() {
                    state.search.previous_source = Self::route_from_state(state);
                }
                state.search.query.clear();
                state.search.submitted_query = None;
                state.search.condition_group = Some(group.clone());
                state.selected_folder = None;
                state.selected_label = None;
            }
        }

        if !matches!(self, Self::Search(_) | Self::AdvancedSearch(_)) {
            state.search = SearchState::default();
        }
    }

    fn load_context(&self) -> String {
        match self {
            Self::Folder(folder_id) => format!("list messages for {folder_id}"),
            Self::Label(label_id) => format!("list messages for label {label_id}"),
            Self::SmartFolder(kind) => format!("list smart folder {kind}"),
            Self::Search(query) => format!("search messages for {query}"),
            Self::AdvancedSearch(_) => "advanced search messages".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
struct LoadedMessageCollection {
    messages: Vec<Message>,
    selected_message: Option<String>,
    selected_full_message: Option<FullMessage>,
    selected_message_labels: Vec<Label>,
}

#[derive(Clone, Debug)]
struct LoadedFullMessage {
    message: FullMessage,
    labels: Vec<Label>,
}

#[derive(Clone, Debug)]
struct NativeSyncRefreshRequest {
    accounts_request: ResourceRequest,
    messages_request: Option<ResourceRequest>,
    source: Option<NativeMessageSource>,
    selected_message: Option<String>,
}

#[derive(Clone, Debug)]
struct LoadedSyncRefresh {
    accounts: Vec<Account>,
    folders_by_account: BTreeMap<String, Vec<Folder>>,
    labels: Vec<Label>,
    messages: Option<LoadedMessageCollection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NativeMessageMutation {
    Archive {
        message_id: String,
    },
    Delete {
        message_id: String,
    },
    SetRead {
        message_id: String,
        is_read: bool,
    },
    SetFlagged {
        message_id: String,
        is_flagged: bool,
    },
    Snooze {
        message_id: String,
        until: String,
    },
}

impl NativeMessageMutation {
    fn message_id(&self) -> &str {
        match self {
            Self::Archive { message_id }
            | Self::Delete { message_id }
            | Self::SetRead { message_id, .. }
            | Self::SetFlagged { message_id, .. }
            | Self::Snooze { message_id, .. } => message_id,
        }
    }

    fn action_label(&self) -> &'static str {
        match self {
            Self::Archive { .. } => "archive message",
            Self::Delete { .. } => "delete message",
            Self::SetRead { is_read: true, .. } => "mark message read",
            Self::SetRead { is_read: false, .. } => "mark message unread",
            Self::SetFlagged {
                is_flagged: true, ..
            } => "flag message",
            Self::SetFlagged {
                is_flagged: false, ..
            } => "unflag message",
            Self::Snooze { .. } => "snooze message",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NativeDraftSeed {
    recipients: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComposeRecipientField {
    To,
    Cc,
    Bcc,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NativeLabelMutation {
    Add {
        message_id: String,
        label_id: String,
    },
    Remove {
        message_id: String,
        label_id: String,
    },
}

impl NativeLabelMutation {
    fn message_id(&self) -> &str {
        match self {
            Self::Add { message_id, .. } | Self::Remove { message_id, .. } => message_id,
        }
    }
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
        .spawn(async move { load_message_collection(base_url, load_source, None) });
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
    state.selected_message_ids.clear();
    state.selected_full_message = None;
    state.selected_message_resource.reset();
    state.selected_message_labels.clear();
    state.selected_message_labels_resource.reset();
    state.last_error = None;
    state.messages_resource.begin_load()
}

fn load_message_collection(
    base_url: String,
    source: NativeMessageSource,
    preferred_message_id: Option<String>,
) -> Result<LoadedMessageCollection> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        load_message_collection_with_client(&client, source, preferred_message_id).await
    })
}

async fn load_message_collection_with_client(
    client: &EmailClient,
    source: NativeMessageSource,
    preferred_message_id: Option<String>,
) -> Result<LoadedMessageCollection> {
    let mut messages = match &source {
        NativeMessageSource::Folder(folder_id) => client.list_folder_messages(folder_id).await,
        NativeMessageSource::Label(label_id) => client.list_label_messages(label_id).await,
        NativeMessageSource::SmartFolder(kind) => client.list_smart_messages(kind).await,
        NativeMessageSource::Search(query) => client
            .search(query, None)
            .await
            .map(|results| results.messages),
        NativeMessageSource::AdvancedSearch(group) => client
            .advanced_search(group)
            .await
            .map(|results| results.messages),
    }
    .with_context(|| source.load_context())?;

    let selected_message = preferred_message_id
        .filter(|message_id| messages.iter().any(|message| message.id == *message_id))
        .or_else(|| messages.first().map(|message| message.id.clone()));
    let (selected_full_message, selected_message_labels) =
        if let Some(message_id) = selected_message.as_deref() {
            match client.get_message(&message_id).await {
                Ok(full_message) => {
                    update_summary_from_full_message(&mut messages, &full_message);
                    let labels = client
                        .get_message_labels(message_id)
                        .await
                        .with_context(|| format!("get labels for selected message {message_id}"))?;
                    (Some(full_message), labels)
                }
                Err(err) => {
                    tracing::warn!("failed to load selected message {message_id}: {err}");
                    (None, Vec::new())
                }
            }
        } else {
            (None, Vec::new())
        };

    Ok(LoadedMessageCollection {
        messages,
        selected_message,
        selected_full_message,
        selected_message_labels,
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
    retain_existing_message_selection(state);
    state.selected_message = loaded
        .selected_message
        .or_else(|| state.messages.first().map(|message| message.id.clone()));
    state.selected_full_message = None;
    state.selected_message_resource.reset();
    state.selected_message_labels.clear();
    state.selected_message_labels_resource.reset();

    if let Some(full_message) = loaded.selected_full_message {
        state.selected_message = Some(full_message.id.clone());
        state.selected_message_resource = AsyncResource::ready(full_message.clone());
        apply_full_message(state, full_message);
        state.selected_message_labels = loaded.selected_message_labels.clone();
        state.selected_message_labels_resource =
            AsyncResource::ready(loaded.selected_message_labels);
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
    state.selected_message_ids.clear();
    state.selected_full_message = None;
    state.selected_message_resource.reset();
    state.selected_message_labels.clear();
    state.selected_message_labels_resource.reset();
    state.last_error = Some(message);
    true
}

fn start_sync_complete_refresh(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    let (base_url, request) = state_entity.update(app_cx, |state, cx| {
        let request = begin_sync_complete_refresh(state);
        sync_window_title(state, cx);
        cx.notify();
        (state.api_base_url.clone(), request)
    });

    let load_request = request.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { load_sync_complete_refresh(base_url, load_request) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_loaded_sync_refresh(state, request, loaded);
                        }
                        Err(err) => {
                            reject_sync_refresh(state, request, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native sync refresh: {err}");
                }
            }
        })
        .detach();
}

fn start_mark_folder_read(
    state_entity: &Entity<NativeAppState>,
    folder_id: String,
    app_cx: &mut App,
) {
    let (base_url, request) = state_entity.update(app_cx, |state, cx| {
        let request = begin_sync_complete_refresh(state);
        state.last_error = None;
        cx.notify();
        (state.api_base_url.clone(), request)
    });

    let load_request = request.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { mark_folder_read_and_refresh(base_url, folder_id, load_request) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_loaded_sync_refresh(state, request, loaded);
                        }
                        Err(err) => {
                            reject_sync_refresh(state, request, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native mark-folder-read refresh: {err}");
                }
            }
        })
        .detach();
}

fn start_message_mutation_and_refresh(
    state_entity: &Entity<NativeAppState>,
    mutation: NativeMessageMutation,
    app_cx: &mut App,
) {
    let (base_url, request) = state_entity.update(app_cx, |state, cx| {
        let request = begin_sync_complete_refresh(state);
        state.last_error = None;
        cx.notify();
        (state.api_base_url.clone(), request)
    });

    let load_request = request.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { mutate_message_and_refresh(base_url, mutation, load_request) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_loaded_sync_refresh(state, request, loaded);
                        }
                        Err(err) => {
                            reject_sync_refresh(state, request, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native message mutation refresh: {err}");
                }
            }
        })
        .detach();
}

fn start_message_label_mutation(
    state_entity: &Entity<NativeAppState>,
    mutation: NativeLabelMutation,
    app_cx: &mut App,
) {
    let message_id = mutation.message_id().to_string();
    let Some((base_url, request)) = state_entity.update(app_cx, |state, cx| {
        if state.selected_message.as_deref() != Some(message_id.as_str()) {
            return None;
        }
        state.last_error = None;
        let request = state.selected_message_labels_resource.begin_load();
        cx.notify();
        Some((state.api_base_url.clone(), request))
    }) else {
        return;
    };

    let task = app_cx
        .background_executor()
        .spawn(async move { mutate_message_label_and_reload(base_url, mutation) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    if state.selected_message.as_deref() != Some(message_id.as_str()) {
                        return;
                    }
                    match result {
                        Ok(labels) => {
                            if state
                                .selected_message_labels_resource
                                .resolve(request, labels.clone())
                            {
                                state.selected_message_labels = labels;
                                state.last_error = None;
                            }
                        }
                        Err(err) => {
                            let message = err.to_string();
                            if state
                                .selected_message_labels_resource
                                .reject(request, message.clone())
                            {
                                state.last_error = Some(message);
                            }
                        }
                    }
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native message label mutation: {err}");
                }
            }
        })
        .detach();
}

fn start_bulk_message_action(
    state_entity: &Entity<NativeAppState>,
    action: BulkAction,
    app_cx: &mut App,
) {
    let Some((base_url, request, ids)) = state_entity.update(app_cx, |state, cx| {
        let ids = selected_action_message_ids(state, None);
        if ids.is_empty() {
            return None;
        }
        let request = begin_sync_complete_refresh(state);
        state.selected_message_ids.clear();
        state.last_error = None;
        cx.notify();
        Some((state.api_base_url.clone(), request, ids))
    }) else {
        return;
    };

    let load_request = request.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { bulk_message_action_and_refresh(base_url, ids, action, load_request) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_loaded_sync_refresh(state, request, loaded);
                        }
                        Err(err) => {
                            reject_sync_refresh(state, request, err);
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native bulk message action refresh: {err}");
                }
            }
        })
        .detach();
}

fn begin_sync_complete_refresh(state: &mut NativeAppState) -> NativeSyncRefreshRequest {
    let source = NativeMessageSource::from_state(state);
    let selected_message = state.selected_message.clone();
    let accounts_request = state.accounts_resource.begin_load();
    let messages_request = source
        .as_ref()
        .map(|_| state.messages_resource.begin_load());
    state.last_error = None;

    NativeSyncRefreshRequest {
        accounts_request,
        messages_request,
        source,
        selected_message,
    }
}

fn load_sync_complete_refresh(
    base_url: String,
    request: NativeSyncRefreshRequest,
) -> Result<LoadedSyncRefresh> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        let tree = load_account_tree(&client).await?;
        let messages = if let Some(source) = request.source {
            Some(
                load_message_collection_with_client(&client, source, request.selected_message)
                    .await?,
            )
        } else {
            None
        };

        Ok(LoadedSyncRefresh {
            accounts: tree.accounts,
            folders_by_account: tree.folders_by_account,
            labels: tree.labels,
            messages,
        })
    })
}

fn mark_folder_read_and_refresh(
    base_url: String,
    folder_id: String,
    request: NativeSyncRefreshRequest,
) -> Result<LoadedSyncRefresh> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .mark_folder_read(&folder_id)
            .await
            .with_context(|| format!("mark folder read {folder_id}"))?;
        let tree = load_account_tree(&client).await?;
        let messages = if let Some(source) = request.source {
            Some(
                load_message_collection_with_client(&client, source, request.selected_message)
                    .await?,
            )
        } else {
            None
        };

        Ok(LoadedSyncRefresh {
            accounts: tree.accounts,
            folders_by_account: tree.folders_by_account,
            labels: tree.labels,
            messages,
        })
    })
}

fn mutate_message_and_refresh(
    base_url: String,
    mutation: NativeMessageMutation,
    request: NativeSyncRefreshRequest,
) -> Result<LoadedSyncRefresh> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        let action_label = mutation.action_label();
        match &mutation {
            NativeMessageMutation::Archive { message_id } => {
                client.archive_message(message_id).await?;
            }
            NativeMessageMutation::Delete { message_id } => {
                client.delete_message(message_id).await?;
            }
            NativeMessageMutation::SetRead {
                message_id,
                is_read,
            } => {
                client
                    .patch_message(
                        message_id,
                        &PatchMessageRequest {
                            is_read: Some(*is_read),
                            is_flagged: None,
                        },
                    )
                    .await?;
            }
            NativeMessageMutation::SetFlagged {
                message_id,
                is_flagged,
            } => {
                client
                    .patch_message(
                        message_id,
                        &PatchMessageRequest {
                            is_read: None,
                            is_flagged: Some(*is_flagged),
                        },
                    )
                    .await?;
            }
            NativeMessageMutation::Snooze { message_id, until } => {
                client
                    .snooze_message(
                        message_id,
                        &SnoozeRequest {
                            until: until.clone(),
                        },
                    )
                    .await?;
            }
        }

        let tree = load_account_tree(&client)
            .await
            .with_context(|| format!("refresh after {action_label}"))?;
        let messages = if let Some(source) = request.source {
            Some(
                load_message_collection_with_client(&client, source, request.selected_message)
                    .await
                    .with_context(|| format!("reload active mailbox after {action_label}"))?,
            )
        } else {
            None
        };

        tracing::info!(
            "completed native {action_label} for {}",
            mutation.message_id()
        );
        Ok(LoadedSyncRefresh {
            accounts: tree.accounts,
            folders_by_account: tree.folders_by_account,
            labels: tree.labels,
            messages,
        })
    })
}

fn bulk_message_action_and_refresh(
    base_url: String,
    ids: Vec<String>,
    action: BulkAction,
    request: NativeSyncRefreshRequest,
) -> Result<LoadedSyncRefresh> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        let action_label = bulk_action_label(&action);
        client
            .bulk_messages(&BulkRequest { ids, action })
            .await
            .with_context(|| format!("bulk {action_label}"))?;

        let tree = load_account_tree(&client)
            .await
            .with_context(|| format!("refresh after bulk {action_label}"))?;
        let messages = if let Some(source) = request.source {
            Some(
                load_message_collection_with_client(&client, source, request.selected_message)
                    .await
                    .with_context(|| format!("reload active mailbox after bulk {action_label}"))?,
            )
        } else {
            None
        };

        Ok(LoadedSyncRefresh {
            accounts: tree.accounts,
            folders_by_account: tree.folders_by_account,
            labels: tree.labels,
            messages,
        })
    })
}

fn bulk_action_label(action: &BulkAction) -> &'static str {
    match action {
        BulkAction::Archive => "archive",
        BulkAction::Delete => "delete",
        BulkAction::MarkRead => "mark read",
        BulkAction::MarkUnread => "mark unread",
    }
}

fn mutate_message_label_and_reload(
    base_url: String,
    mutation: NativeLabelMutation,
) -> Result<Vec<Label>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        match &mutation {
            NativeLabelMutation::Add {
                message_id,
                label_id,
            } => {
                client
                    .add_message_label(message_id, label_id)
                    .await
                    .with_context(|| format!("add label {label_id} to message {message_id}"))?;
            }
            NativeLabelMutation::Remove {
                message_id,
                label_id,
            } => {
                client
                    .remove_message_label(message_id, label_id)
                    .await
                    .with_context(|| {
                        format!("remove label {label_id} from message {message_id}")
                    })?;
            }
        }

        client
            .get_message_labels(mutation.message_id())
            .await
            .with_context(|| format!("reload labels for message {}", mutation.message_id()))
    })
}

fn apply_loaded_sync_refresh(
    state: &mut NativeAppState,
    request: NativeSyncRefreshRequest,
    loaded: LoadedSyncRefresh,
) -> bool {
    if !state
        .accounts_resource
        .resolve(request.accounts_request, loaded.accounts.clone())
    {
        return false;
    }

    state.accounts = loaded.accounts;
    state.folders_by_account = loaded.folders_by_account;
    state.labels = loaded.labels;

    if let (Some(messages_request), Some(messages)) = (request.messages_request, loaded.messages) {
        apply_loaded_message_collection(state, messages_request, messages);
    }

    state.last_error = None;
    true
}

fn reject_sync_refresh(
    state: &mut NativeAppState,
    request: NativeSyncRefreshRequest,
    err: anyhow::Error,
) -> bool {
    let message = err.to_string();
    let accounts_rejected = state
        .accounts_resource
        .reject(request.accounts_request, message.clone());
    let messages_rejected = request.messages_request.is_some_and(|messages_request| {
        state
            .messages_resource
            .reject(messages_request, message.clone())
    });

    if !accounts_rejected && !messages_rejected {
        return false;
    }

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
                        Ok(loaded) => {
                            apply_loaded_full_message(state, request, &message_id, loaded);
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

    if state.selected_message.as_deref() != Some(message_id) {
        reset_compose_state(state, None);
    }
    state.selected_message = Some(message_id.to_string());
    state.selected_full_message = None;
    state.selected_message_labels.clear();
    state.selected_message_labels_resource.reset();
    state.last_error = None;
    Some(state.selected_message_resource.begin_load())
}

fn load_full_message(base_url: String, message_id: String) -> Result<LoadedFullMessage> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        let message = client
            .get_message(&message_id)
            .await
            .with_context(|| format!("get message {message_id}"))?;
        let labels = client
            .get_message_labels(&message_id)
            .await
            .with_context(|| format!("get labels for message {message_id}"))?;
        Ok(LoadedFullMessage { message, labels })
    })
}

fn apply_loaded_full_message(
    state: &mut NativeAppState,
    request: ResourceRequest,
    message_id: &str,
    loaded: LoadedFullMessage,
) -> bool {
    if state.selected_message.as_deref() != Some(message_id) {
        return false;
    }
    if !state
        .selected_message_resource
        .resolve(request, loaded.message.clone())
    {
        return false;
    }

    apply_full_message(state, loaded.message);
    state.selected_message_labels = loaded.labels.clone();
    state.selected_message_labels_resource = AsyncResource::ready(loaded.labels);
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
    state.selected_message_labels.clear();
    state.selected_message_labels_resource.reset();
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

fn selected_action_message_ids(
    state: &NativeAppState,
    fallback_message_id: Option<&str>,
) -> Vec<String> {
    let visible = state
        .messages
        .iter()
        .map(|message| message.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut ids = Vec::new();
    let mut seen = BTreeSet::new();

    for id in &state.selected_message_ids {
        if visible.contains(id.as_str()) && seen.insert(id.clone()) {
            ids.push(id.clone());
        }
    }

    if ids.is_empty() {
        if let Some(id) = fallback_message_id {
            if visible.contains(id) {
                ids.push(id.to_string());
            }
        }
    }

    ids
}

fn toggle_message_selection(state: &mut NativeAppState, message_id: &str) {
    if let Some(pos) = state
        .selected_message_ids
        .iter()
        .position(|selected_id| selected_id == message_id)
    {
        state.selected_message_ids.remove(pos);
    } else if state
        .messages
        .iter()
        .any(|message| message.id == message_id)
    {
        state.selected_message_ids.push(message_id.to_string());
    }
}

fn set_all_message_selection(state: &mut NativeAppState, selected: bool) {
    if selected {
        state.selected_message_ids = state
            .messages
            .iter()
            .map(|message| message.id.clone())
            .collect();
    } else {
        state.selected_message_ids.clear();
    }
}

fn retain_existing_message_selection(state: &mut NativeAppState) {
    let visible = state
        .messages
        .iter()
        .map(|message| message.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    state
        .selected_message_ids
        .retain(|id| visible.contains(id.as_str()) && seen.insert(id.clone()));
}

fn message_select_all_state(state: &NativeAppState) -> CheckboxState {
    if state.messages.is_empty() || state.selected_message_ids.is_empty() {
        return CheckboxState::Unchecked;
    }

    let visible_count = state.messages.len();
    let selected_visible_count = selected_action_message_ids(state, None).len();
    if selected_visible_count == 0 {
        CheckboxState::Unchecked
    } else if selected_visible_count == visible_count {
        CheckboxState::Checked
    } else {
        CheckboxState::Indeterminate
    }
}

fn start_new_compose(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    let load = state_entity.update(app_cx, |state, cx| {
        let Some(account_id) = state.accounts.first().map(|account| account.id.clone()) else {
            let message = "Add an account before composing a message.".to_string();
            state.compose.error = Some(message.clone());
            state.last_error = Some(message);
            cx.notify();
            return None;
        };

        state.compose = ComposeState {
            open: true,
            mode: Some("compose".to_string()),
            account_id: Some(account_id.clone()),
            revision: state.compose.revision.saturating_add(1),
            signature_loading: true,
            ..ComposeState::default()
        };
        state.selected_view = SelectedView::Mail;
        state.last_error = None;
        cx.notify();
        Some((
            state.api_base_url.clone(),
            account_id,
            state.compose.revision,
        ))
    });

    if let Some((base_url, account_id, revision)) = load {
        start_compose_signature_load(state_entity, base_url, account_id, revision, app_cx);
    }
}

fn start_reply_draft(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    let load = state_entity.update(app_cx, |state, cx| {
        let Some(seed) = selected_reply_seed(state) else {
            let message = "Select a message before replying.".to_string();
            state.compose.error = Some(message.clone());
            state.last_error = Some(message);
            cx.notify();
            return None;
        };
        let Some(account_id) = selected_account_id(state) else {
            let message = "No account is available for this message.".to_string();
            state.compose.error = Some(message.clone());
            state.last_error = Some(message);
            cx.notify();
            return None;
        };

        state.compose = ComposeState {
            open: true,
            mode: Some("reply".to_string()),
            account_id: Some(account_id.clone()),
            to: seed
                .recipients
                .into_iter()
                .map(|(value, _)| value)
                .collect(),
            subject: Some(reply_subject(selected_subject(state))),
            body: String::new(),
            in_reply_to: selected_in_reply_to(state),
            quoted_text: selected_quote_text(state),
            quoted_from: Some(selected_sender(state)),
            signature_loading: true,
            revision: state.compose.revision.saturating_add(1),
            ..ComposeState::default()
        };
        state.selected_view = SelectedView::Mail;
        state.last_error = None;
        cx.notify();
        Some((
            state.api_base_url.clone(),
            account_id,
            state.compose.revision,
        ))
    });

    if let Some((base_url, account_id, revision)) = load {
        start_compose_signature_load(state_entity, base_url, account_id, revision, app_cx);
    }
}

fn start_forward_draft(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    let load = state_entity.update(app_cx, |state, cx| {
        if state.selected_message.is_none() {
            let message = "Select a message before forwarding.".to_string();
            state.compose.error = Some(message.clone());
            state.last_error = Some(message);
            cx.notify();
            return None;
        }
        let Some(account_id) = selected_account_id(state) else {
            let message = "No account is available for this message.".to_string();
            state.compose.error = Some(message.clone());
            state.last_error = Some(message);
            cx.notify();
            return None;
        };

        state.compose = ComposeState {
            open: true,
            mode: Some("forward".to_string()),
            account_id: Some(account_id.clone()),
            subject: Some(forward_subject(selected_subject(state))),
            body: selected_forward_body(state),
            quoted_text: selected_quote_text(state),
            quoted_from: Some(selected_sender(state)),
            signature_loading: true,
            revision: state.compose.revision.saturating_add(1),
            ..ComposeState::default()
        };
        state.selected_view = SelectedView::Mail;
        state.last_error = None;
        cx.notify();
        Some((
            state.api_base_url.clone(),
            account_id,
            state.compose.revision,
        ))
    });

    if let Some((base_url, account_id, revision)) = load {
        start_compose_signature_load(state_entity, base_url, account_id, revision, app_cx);
    }
}

fn start_compose_signature_load(
    state_entity: &Entity<NativeAppState>,
    base_url: String,
    account_id: String,
    revision: u64,
    app_cx: &mut App,
) {
    let task = app_cx.background_executor().spawn(async move {
        load_account_signature(base_url, account_id.clone())
            .map(|signature| (account_id, signature))
    });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    if state.compose.revision != revision || !state.compose.open {
                        return;
                    }
                    state.compose.signature_loading = false;
                    match result {
                        Ok((account_id, settings)) => {
                            if state.compose.account_id.as_deref() == Some(account_id.as_str()) {
                                state.compose.signature = settings
                                    .signature
                                    .filter(|signature| !signature.trim().is_empty());
                            }
                        }
                        Err(err) => {
                            tracing::warn!("failed to load native compose signature: {err:#}");
                        }
                    }
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native compose signature: {err}");
                }
            }
        })
        .detach();
}

fn load_account_signature(base_url: String, account_id: String) -> Result<AccountSettings> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .account_settings(&account_id)
            .await
            .with_context(|| format!("get account settings for {account_id}"))
    })
}

fn update_compose_recipients(
    state_entity: &Entity<NativeAppState>,
    field: ComposeRecipientField,
    tokens: Vec<TokenInputToken>,
    app_cx: &mut App,
) {
    let values = tokens
        .into_iter()
        .filter(|token| token.valid)
        .map(|token| token.value.to_string())
        .collect::<Vec<_>>();
    state_entity.update(app_cx, |state, cx| {
        match field {
            ComposeRecipientField::To => state.compose.to = values,
            ComposeRecipientField::Cc => state.compose.cc = values,
            ComposeRecipientField::Bcc => state.compose.bcc = values,
        }
        cx.notify();
    });
}

fn update_compose_subject(
    state_entity: &Entity<NativeAppState>,
    subject: String,
    app_cx: &mut App,
) {
    state_entity.update(app_cx, |state, cx| {
        state.compose.subject = Some(subject);
        cx.notify();
    });
}

fn update_compose_body(state_entity: &Entity<NativeAppState>, body: String, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        state.compose.body = body;
        cx.notify();
    });
}

fn compose_tokens(values: &[String]) -> Vec<TokenInputToken> {
    values
        .iter()
        .map(|value| TokenInputToken::new(value.clone(), value.clone()))
        .collect()
}

fn start_snooze_from_picker(
    state_entity: &Entity<NativeAppState>,
    snooze_picker: Entity<DateTimePicker>,
    message_id: String,
    app_cx: &mut App,
) {
    let value = snooze_picker
        .read(app_cx)
        .selected_value()
        .unwrap_or_else(default_snooze_value);
    let until = match date_time_value_to_utc_iso(value) {
        Ok(until) => until,
        Err(err) => {
            state_entity.update(app_cx, |state, cx| {
                state.last_error = Some(err.to_string());
                cx.notify();
            });
            return;
        }
    };

    start_message_mutation_and_refresh(
        state_entity,
        NativeMessageMutation::Snooze { message_id, until },
        app_cx,
    );
}

fn start_quick_reply_send(
    state_entity: &Entity<NativeAppState>,
    recipient_input: Entity<TokenInput>,
    quick_reply_input: Entity<TextInput>,
    app_cx: &mut App,
) {
    let recipients = recipient_input
        .read(app_cx)
        .values()
        .iter()
        .filter(|token| token.valid)
        .map(|token| token.value.to_string())
        .collect::<Vec<_>>();
    let body = quick_reply_input.read(app_cx).text().trim().to_string();

    let Some((base_url, request)) = state_entity.update(app_cx, |state, cx| {
        match build_quick_reply_request(state, recipients, body) {
            Ok(request) => {
                state.compose.sending = true;
                state.compose.error = None;
                state.compose.notice = None;
                state.last_error = None;
                cx.notify();
                Some((state.api_base_url.clone(), request))
            }
            Err(message) => {
                state.compose.error = Some(message.clone());
                state.last_error = Some(message);
                cx.notify();
                None
            }
        }
    }) else {
        return;
    };

    let task = app_cx
        .background_executor()
        .spawn(async move { send_quick_reply(base_url, request) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let sent = result.is_ok();
                let error = result.err().map(|err| err.to_string());

                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    state.compose.sending = false;
                    if sent {
                        state.compose.open = false;
                        state.compose.error = None;
                        state.compose.notice = Some("Reply sent".to_string());
                        state.last_error = None;
                    } else if let Some(error) = error.clone() {
                        state.compose.error = Some(error.clone());
                        state.last_error = Some(error);
                    }
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native quick reply status: {err}");
                }

                if sent {
                    let _ = quick_reply_input.update(&mut async_cx, |input, cx| {
                        input.set_value("", cx);
                    });
                    let _ = recipient_input.update(&mut async_cx, |input, cx| {
                        input.set_tokens(Vec::<TokenInputToken>::new(), cx);
                    });
                }
            }
        })
        .detach();
}

fn start_compose_send(
    state_entity: &Entity<NativeAppState>,
    to_input: Entity<TokenInput>,
    cc_input: Entity<TokenInput>,
    bcc_input: Entity<TokenInput>,
    subject_input: Entity<TextInput>,
    body_input: Entity<Textarea>,
    app_cx: &mut App,
) {
    let to = token_input_values(&to_input, app_cx);
    let cc = token_input_values(&cc_input, app_cx);
    let bcc = token_input_values(&bcc_input, app_cx);
    let subject = subject_input.read(app_cx).text().to_string();
    let body = body_input.read(app_cx).text().to_string();

    let Some((base_url, request)) = state_entity.update(app_cx, |state, cx| {
        match build_compose_request(state, to, cc, bcc, subject, body) {
            Ok(request) => {
                state.compose.sending = true;
                state.compose.error = None;
                state.compose.notice = None;
                state.last_error = None;
                cx.notify();
                Some((state.api_base_url.clone(), request))
            }
            Err(message) => {
                state.compose.error = Some(message.clone());
                state.last_error = Some(message);
                cx.notify();
                None
            }
        }
    }) else {
        return;
    };

    let task = app_cx
        .background_executor()
        .spawn(async move { send_message_request(base_url, request) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                let sent = result.is_ok();
                let error = result.err().map(|err| err.to_string());

                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    state.compose.sending = false;
                    if sent {
                        reset_compose_state(state, Some("Message sent".to_string()));
                        state.last_error = None;
                    } else if let Some(error) = error.clone() {
                        state.compose.error = Some(error.clone());
                        state.last_error = Some(error);
                    }
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native compose send status: {err}");
                }

                if sent {
                    let _ = to_input.update(&mut async_cx, |input, cx| {
                        input.set_tokens(Vec::<TokenInputToken>::new(), cx);
                    });
                    let _ = cc_input.update(&mut async_cx, |input, cx| {
                        input.set_tokens(Vec::<TokenInputToken>::new(), cx);
                    });
                    let _ = bcc_input.update(&mut async_cx, |input, cx| {
                        input.set_tokens(Vec::<TokenInputToken>::new(), cx);
                    });
                    let _ = subject_input.update(&mut async_cx, |input, cx| {
                        input.set_value("", cx);
                    });
                    let _ = body_input.update(&mut async_cx, |input, cx| {
                        input.set_value("", cx);
                    });
                }
            }
        })
        .detach();
}

fn discard_compose(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        reset_compose_state(state, None);
        state.last_error = None;
        cx.notify();
    });
}

fn token_input_values(input: &Entity<TokenInput>, app_cx: &App) -> Vec<String> {
    input
        .read(app_cx)
        .values()
        .iter()
        .filter(|token| token.valid)
        .map(|token| token.value.to_string())
        .collect()
}

fn reset_compose_state(state: &mut NativeAppState, notice: Option<String>) {
    let revision = state.compose.revision.saturating_add(1);
    state.compose = ComposeState {
        revision,
        notice,
        ..ComposeState::default()
    };
}

fn send_quick_reply(base_url: String, request: SendRequest) -> Result<()> {
    send_message_request(base_url, request)
}

fn send_message_request(base_url: String, request: SendRequest) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client.send_message(&request).await?;
        Ok(())
    })
}

fn build_quick_reply_request(
    state: &NativeAppState,
    recipients: Vec<String>,
    body: String,
) -> std::result::Result<SendRequest, String> {
    let body = body.trim().to_string();
    if body.is_empty() {
        return Err("Write a message before sending.".to_string());
    }

    let mode = state.compose.mode.as_deref().unwrap_or("reply");
    let mut to = normalize_recipients(recipients);
    if to.is_empty() && mode != "forward" {
        if let Some(email) = selected_from_email(state) {
            to.push(email);
        }
    }
    if to.is_empty() {
        return Err("Add at least one recipient.".to_string());
    }

    let account_id = state
        .compose
        .account_id
        .clone()
        .or_else(|| selected_account_id(state))
        .ok_or_else(|| "No account is available for this message.".to_string())?;
    let subject = state.compose.subject.clone().unwrap_or_else(|| {
        if mode == "forward" {
            forward_subject(selected_subject(state))
        } else {
            reply_subject(selected_subject(state))
        }
    });
    let in_reply_to = if mode == "forward" {
        None
    } else {
        state
            .compose
            .in_reply_to
            .clone()
            .or_else(|| selected_in_reply_to(state))
    };

    Ok(SendRequest {
        account_id,
        to,
        cc: Vec::new(),
        bcc: Vec::new(),
        subject,
        text_body: Some(body),
        html_body: None,
        in_reply_to,
    })
}

fn build_compose_request(
    state: &NativeAppState,
    to: Vec<String>,
    cc: Vec<String>,
    bcc: Vec<String>,
    subject: String,
    body: String,
) -> std::result::Result<SendRequest, String> {
    let mode = state.compose.mode.as_deref().unwrap_or("compose");
    if state.compose.signature_loading {
        return Err("Wait for the account signature to load.".to_string());
    }

    let mut to = normalize_recipients(to);
    if to.is_empty() && mode == "reply" {
        if let Some(email) = selected_from_email(state) {
            to.push(email);
        }
    }
    if to.is_empty() {
        return Err("Add at least one recipient.".to_string());
    }

    let text_body = build_compose_text_body(state, mode, body)?;
    let account_id = state
        .compose
        .account_id
        .clone()
        .or_else(|| selected_account_id(state))
        .or_else(|| state.accounts.first().map(|account| account.id.clone()))
        .ok_or_else(|| "No account is available for this message.".to_string())?;
    let subject = subject.trim().to_string();
    let in_reply_to = if mode == "reply" {
        state
            .compose
            .in_reply_to
            .clone()
            .or_else(|| selected_in_reply_to(state))
    } else {
        None
    };

    Ok(SendRequest {
        account_id,
        to,
        cc: normalize_recipients(cc),
        bcc: normalize_recipients(bcc),
        subject,
        text_body: Some(text_body),
        html_body: None,
        in_reply_to,
    })
}

fn build_compose_text_body(
    state: &NativeAppState,
    mode: &str,
    body: String,
) -> std::result::Result<String, String> {
    let mut text_body = body.trim().to_string();
    if text_body.is_empty() {
        return Err("Write a message before sending.".to_string());
    }

    if mode == "reply" {
        if let Some(quote) = state
            .compose
            .quoted_text
            .as_deref()
            .map(str::trim)
            .filter(|quote| !quote.is_empty())
        {
            text_body.push_str("\n\n");
            text_body.push_str(&build_quoted_block(
                quote,
                state.compose.quoted_from.as_deref(),
            ));
        }
    }

    if let Some(signature) = state
        .compose
        .signature
        .as_deref()
        .map(str::trim)
        .filter(|signature| !signature.is_empty())
    {
        text_body.push_str("\n\n-- \n");
        text_body.push_str(signature);
    }

    Ok(text_body)
}

fn build_quoted_block(text: &str, from: Option<&str>) -> String {
    let prefix = from
        .map(str::trim)
        .filter(|from| !from.is_empty())
        .map(|from| format!("{from} wrote:\n"))
        .unwrap_or_else(|| "Original message:\n".to_string());
    format!(
        "{prefix}{}",
        text.lines()
            .map(|line| format!("> {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn build_forward_body(text: &str, from: Option<&str>) -> String {
    let header = from
        .map(str::trim)
        .filter(|from| !from.is_empty())
        .map(|from| format!("---------- Forwarded message from {from} ----------\n\n"))
        .unwrap_or_else(|| "---------- Forwarded message ----------\n\n".to_string());
    format!("{header}{}", text.trim())
}

fn selected_reply_seed(state: &NativeAppState) -> Option<NativeDraftSeed> {
    let email = selected_from_email(state)?;
    let label = selected_sender(state);
    Some(NativeDraftSeed {
        recipients: vec![(email, label)],
    })
}

fn selected_account_id(state: &NativeAppState) -> Option<String> {
    state
        .selected_full_message
        .as_ref()
        .map(|message| message.account_id.clone())
        .or_else(|| selected_summary(state).map(|message| message.account_id.clone()))
}

fn selected_in_reply_to(state: &NativeAppState) -> Option<String> {
    state
        .selected_full_message
        .as_ref()
        .and_then(|message| message.message_id.clone())
        .or_else(|| selected_summary(state).and_then(|message| message.message_id.clone()))
}

fn selected_from_email(state: &NativeAppState) -> Option<String> {
    state
        .selected_full_message
        .as_ref()
        .and_then(|message| message.from_email.clone())
        .or_else(|| selected_summary(state).and_then(|message| message.from_email.clone()))
        .map(|email| email.trim().to_string())
        .filter(|email| !email.is_empty())
}

fn selected_forward_body(state: &NativeAppState) -> String {
    let body = selected_quote_text(state).unwrap_or_default();

    if body.is_empty() {
        return String::new();
    }

    build_forward_body(&body, Some(&selected_sender(state)))
}

fn selected_quote_text(state: &NativeAppState) -> Option<String> {
    state
        .selected_full_message
        .as_ref()
        .and_then(|message| message.body.as_ref())
        .and_then(|body| body.text_body.as_ref())
        .or_else(|| selected_summary(state).and_then(|message| message.preview.as_ref()))
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn reply_subject(subject: String) -> String {
    if subject.trim_start().to_ascii_lowercase().starts_with("re:") {
        subject
    } else {
        format!("Re: {subject}")
    }
}

fn forward_subject(subject: String) -> String {
    if subject
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("fwd:")
    {
        subject
    } else {
        format!("Fwd: {subject}")
    }
}

fn normalize_recipients(recipients: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    recipients
        .into_iter()
        .map(|recipient| recipient.trim().to_string())
        .filter(|recipient| !recipient.is_empty())
        .filter(|recipient| seen.insert(recipient.to_ascii_lowercase()))
        .collect()
}

fn date_time_value_to_utc_iso(value: DateTimeValue) -> Result<String> {
    let local = Local
        .with_ymd_and_hms(
            value.year,
            value.month.into(),
            value.day.into(),
            value.hour.into(),
            value.minute.into(),
            0,
        )
        .single()
        .ok_or_else(|| anyhow!("invalid local snooze time {}", value.display_value()))?;
    Ok(local.with_timezone(&Utc).to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;
    use email_types::{MessageBody, RuleActionDto, RuleConditionDto};

    fn account(id: &str) -> Account {
        Account {
            id: id.to_string(),
            name: "Account".to_string(),
            email: format!("{id}@example.test"),
            provider_type: "generic_imap".to_string(),
            auth_type: "basic".to_string(),
            created_at: "2026-05-25T00:00:00Z".to_string(),
        }
    }

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

    fn label(id: &str) -> Label {
        Label {
            id: id.to_string(),
            account_id: "account".to_string(),
            name: id.to_string(),
            color: "#5b5fc7".to_string(),
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
    fn native_preferences_parse_and_clamp_density() {
        let preferences =
            parse_native_preferences("\n# comment\ntheme=light\ndensity=22\nignored=value\n");

        assert_eq!(
            preferences,
            NativePreferences {
                theme: ThemeMode::Light,
                density_level: 8,
            }
        );
    }

    #[test]
    fn native_preferences_round_trip_to_disk() {
        let path = std::env::temp_dir().join(format!(
            "email-rs-native-prefs-{}-{}.conf",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let preferences = NativePreferences {
            theme: ThemeMode::Light,
            density_level: 2,
        };

        write_native_preferences(&path, &preferences).expect("write preferences");
        let raw = std::fs::read_to_string(&path).expect("read preferences");
        assert_eq!(parse_native_preferences(&raw), preferences);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn native_message_source_uses_current_selection() {
        let mut state = NativeAppState {
            selected_folder: Some("smart:unread".to_string()),
            ..NativeAppState::default()
        };
        assert_eq!(
            NativeMessageSource::from_state(&state),
            Some(NativeMessageSource::SmartFolder("unread".to_string()))
        );

        state.selected_label = Some("label-a".to_string());
        assert_eq!(
            NativeMessageSource::from_state(&state),
            Some(NativeMessageSource::Label("label-a".to_string()))
        );

        state.selected_label = None;
        state.selected_folder = Some("inbox".to_string());
        assert_eq!(
            NativeMessageSource::from_state(&state),
            Some(NativeMessageSource::Folder("inbox".to_string()))
        );
    }

    #[test]
    fn search_source_records_previous_route_and_updates_title() {
        let mut state = NativeAppState {
            selected_folder: Some("inbox".to_string()),
            ..NativeAppState::default()
        };

        NativeMessageSource::Search("quarterly report".to_string()).apply_selection(&mut state);

        assert_eq!(state.selected_folder, None);
        assert_eq!(state.selected_label, None);
        assert_eq!(state.search.query, "quarterly report");
        assert_eq!(
            state.search.submitted_query.as_deref(),
            Some("quarterly report")
        );
        assert_eq!(
            state.search.previous_source,
            Some(NativeMessageSource::Folder("inbox".to_string()))
        );
        assert_eq!(
            NativeMessageSource::from_state(&state),
            Some(NativeMessageSource::Search("quarterly report".to_string()))
        );
        assert_eq!(
            current_mailbox_title(&state),
            "Results for \"quarterly report\""
        );
    }

    #[test]
    fn advanced_search_source_records_previous_route_and_updates_title() {
        let group = ConditionGroup {
            match_mode: MatchMode::Any,
            conditions: vec![Condition {
                id: "c1".to_string(),
                field: ConditionField::Subject,
                operator: ConditionOperator::StartsWith,
                value: "quarterly".to_string(),
            }],
        };
        let mut state = NativeAppState {
            selected_folder: Some("inbox".to_string()),
            ..NativeAppState::default()
        };

        NativeMessageSource::AdvancedSearch(group.clone()).apply_selection(&mut state);

        assert_eq!(state.selected_folder, None);
        assert_eq!(state.selected_label, None);
        assert_eq!(state.search.query, "");
        assert_eq!(state.search.submitted_query, None);
        assert_eq!(state.search.condition_group, Some(group.clone()));
        assert_eq!(
            state.search.previous_source,
            Some(NativeMessageSource::Folder("inbox".to_string()))
        );
        assert_eq!(
            NativeMessageSource::from_state(&state),
            Some(NativeMessageSource::AdvancedSearch(group))
        );
        assert_eq!(current_mailbox_title(&state), "Advanced search results");
    }

    #[test]
    fn basic_search_clears_advanced_search_state() {
        let mut state = NativeAppState {
            search: SearchState {
                condition_group: Some(ConditionGroup {
                    match_mode: MatchMode::All,
                    conditions: vec![Condition {
                        id: "c1".to_string(),
                        field: ConditionField::From,
                        operator: ConditionOperator::Contains,
                        value: "ada".to_string(),
                    }],
                }),
                ..SearchState::default()
            },
            ..NativeAppState::default()
        };

        NativeMessageSource::Search("status".to_string()).apply_selection(&mut state);

        assert_eq!(state.search.condition_group, None);
        assert_eq!(state.search.submitted_query.as_deref(), Some("status"));
        assert_eq!(current_mailbox_title(&state), "Results for \"status\"");
    }

    #[test]
    fn route_selection_clears_active_search() {
        let mut state = NativeAppState {
            search: SearchState {
                query: "report".to_string(),
                submitted_query: Some("report".to_string()),
                condition_group: None,
                previous_source: Some(NativeMessageSource::Folder("inbox".to_string())),
            },
            ..NativeAppState::default()
        };

        NativeMessageSource::SmartFolder("unread".to_string()).apply_selection(&mut state);

        assert_eq!(state.search, SearchState::default());
        assert_eq!(state.selected_folder.as_deref(), Some("smart:unread"));
        assert_eq!(state.selected_label, None);
    }

    #[test]
    fn selected_regular_folder_id_skips_search_label_and_smart_views() {
        let mut state = NativeAppState {
            selected_folder: Some("inbox".to_string()),
            ..NativeAppState::default()
        };
        assert_eq!(selected_regular_folder_id(&state), Some("inbox"));

        state.selected_folder = Some("smart:unread".to_string());
        assert_eq!(selected_regular_folder_id(&state), None);

        state.selected_folder = Some("inbox".to_string());
        state.selected_label = Some("label".to_string());
        assert_eq!(selected_regular_folder_id(&state), None);

        state.selected_label = None;
        state.search.submitted_query = Some("report".to_string());
        assert_eq!(selected_regular_folder_id(&state), None);
    }

    #[test]
    fn preferred_account_folder_prefers_visible_inbox() {
        let mut archive = folder("archive", 0);
        let mut inbox = folder("inbox", 0);
        inbox.special_use = Some("inbox".to_string());
        let mut state = NativeAppState {
            folders_by_account: BTreeMap::from([(
                "account".to_string(),
                vec![archive.clone(), inbox.clone()],
            )]),
            ..NativeAppState::default()
        };

        assert_eq!(
            preferred_account_folder_id(&state, "account").as_deref(),
            Some("inbox")
        );

        archive.is_excluded = false;
        inbox.is_excluded = true;
        state
            .folders_by_account
            .insert("account".to_string(), vec![archive, inbox]);

        assert_eq!(
            preferred_account_folder_id(&state, "account").as_deref(),
            Some("archive")
        );
    }

    #[test]
    fn selected_palette_message_state_uses_summary_then_full_message() {
        let mut summary = message("message", "inbox", false);
        summary.is_flagged = true;
        let mut state = NativeAppState {
            selected_message: Some("message".to_string()),
            messages: vec![summary],
            selected_full_message: Some(full_message("message", "inbox", true, false)),
            ..NativeAppState::default()
        };

        assert_eq!(
            selected_palette_message_state(&state),
            Some(("message".to_string(), false, true))
        );

        state.messages.clear();
        assert_eq!(
            selected_palette_message_state(&state),
            Some(("message".to_string(), true, false))
        );
    }

    #[test]
    fn message_key_target_tracks_roving_selection() {
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];

        assert_eq!(
            message_key_target(&ids, Some("b"), "down").as_deref(),
            Some("c")
        );
        assert_eq!(
            message_key_target(&ids, Some("b"), "up").as_deref(),
            Some("a")
        );
        assert_eq!(
            message_key_target(&ids, Some("b"), "j").as_deref(),
            Some("c")
        );
        assert_eq!(
            message_key_target(&ids, Some("b"), "k").as_deref(),
            Some("a")
        );
        assert_eq!(
            message_key_target(&ids, Some("b"), "return").as_deref(),
            Some("b")
        );
        assert_eq!(
            message_key_target(&ids, Some("z"), "down").as_deref(),
            Some("b")
        );
        assert_eq!(message_key_target(&ids, Some("b"), "x"), None);
    }

    #[test]
    fn message_selection_helpers_track_visible_rows() {
        let mut state = NativeAppState {
            messages: vec![
                message("a", "inbox", true),
                message("b", "inbox", false),
                message("c", "inbox", true),
            ],
            ..NativeAppState::default()
        };

        assert_eq!(message_select_all_state(&state), CheckboxState::Unchecked);
        toggle_message_selection(&mut state, "b");
        assert_eq!(state.selected_message_ids, vec!["b"]);
        assert_eq!(
            selected_action_message_ids(&state, Some("a")),
            vec!["b".to_string()]
        );
        assert_eq!(
            message_select_all_state(&state),
            CheckboxState::Indeterminate
        );

        set_all_message_selection(&mut state, true);
        assert_eq!(state.selected_message_ids, vec!["a", "b", "c"]);
        assert_eq!(message_select_all_state(&state), CheckboxState::Checked);

        state.messages.remove(1);
        retain_existing_message_selection(&mut state);
        assert_eq!(state.selected_message_ids, vec!["a", "c"]);

        set_all_message_selection(&mut state, false);
        assert!(state.selected_message_ids.is_empty());
    }

    #[test]
    fn sender_initials_use_display_name_or_fallback() {
        assert_eq!(sender_initials("Ada Lovelace"), "AL");
        assert_eq!(sender_initials("alerts@example.test"), "A");
        assert_eq!(sender_initials(""), "U");
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
    fn sync_refresh_preserves_selected_message_when_still_present() {
        let mut state = NativeAppState {
            accounts: vec![account("old")],
            accounts_resource: AsyncResource::ready(vec![account("old")]),
            folders_by_account: BTreeMap::from([("account".to_string(), vec![folder("inbox", 1)])]),
            selected_folder: Some("inbox".to_string()),
            selected_message: Some("selected".to_string()),
            selected_full_message: Some(full_message("selected", "inbox", true, false)),
            messages: vec![message("selected", "inbox", true)],
            messages_resource: AsyncResource::ready(vec![message("selected", "inbox", true)]),
            ..NativeAppState::default()
        };

        let request = begin_sync_complete_refresh(&mut state);
        assert!(state.accounts_resource.is_loading());
        assert!(state.messages_resource.is_loading());
        assert_eq!(
            request.source,
            Some(NativeMessageSource::Folder("inbox".to_string()))
        );

        let loaded = LoadedSyncRefresh {
            accounts: vec![account("account")],
            folders_by_account: BTreeMap::from([("account".to_string(), vec![folder("inbox", 2)])]),
            labels: Vec::new(),
            messages: Some(LoadedMessageCollection {
                messages: vec![
                    message("newer", "inbox", true),
                    message("selected", "inbox", true),
                ],
                selected_message: Some("selected".to_string()),
                selected_full_message: Some(full_message("selected", "inbox", true, false)),
                selected_message_labels: vec![label("important")],
            }),
        };

        assert!(apply_loaded_sync_refresh(&mut state, request, loaded));
        assert_eq!(state.accounts, vec![account("account")]);
        assert_eq!(state.selected_message.as_deref(), Some("selected"));
        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.selected_message_labels, vec![label("important")]);
        assert_eq!(total_unread_count(&state), 2);
        assert!(state.last_error.is_none());
    }

    #[test]
    fn stale_sync_refresh_is_ignored() {
        let mut state = NativeAppState {
            accounts_resource: AsyncResource::ready(vec![account("old")]),
            selected_folder: Some("inbox".to_string()),
            messages_resource: AsyncResource::ready(vec![message("old", "inbox", true)]),
            ..NativeAppState::default()
        };

        let stale = begin_sync_complete_refresh(&mut state);
        let _current = begin_sync_complete_refresh(&mut state);
        let loaded = LoadedSyncRefresh {
            accounts: vec![account("new")],
            folders_by_account: BTreeMap::from([("account".to_string(), vec![folder("inbox", 4)])]),
            labels: Vec::new(),
            messages: Some(LoadedMessageCollection {
                messages: vec![message("new", "inbox", true)],
                selected_message: Some("new".to_string()),
                selected_full_message: Some(full_message("new", "inbox", true, false)),
                selected_message_labels: Vec::new(),
            }),
        };

        assert!(!apply_loaded_sync_refresh(&mut state, stale, loaded));
        assert!(state.accounts.is_empty());
        assert!(state.messages.is_empty());
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
        state.selected_message_labels = vec![label("old-label")];
        state.selected_message_labels_resource = AsyncResource::ready(vec![label("old-label")]);

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
        assert!(state.selected_message_labels.is_empty());
        assert!(state.selected_message_labels_resource.is_idle());
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
            selected_message: Some("stale".to_string()),
            selected_full_message: None,
            selected_message_labels: Vec::new(),
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
            selected_message: Some("message".to_string()),
            selected_full_message: Some(full_message("message", "inbox", true, true)),
            selected_message_labels: vec![label("work")],
        };

        assert!(apply_loaded_message_collection(&mut state, request, loaded));
        assert_eq!(state.selected_message.as_deref(), Some("message"));
        assert!(state.selected_full_message.is_some());
        assert!(state.selected_message_resource.is_ready());
        assert!(state.messages[0].is_read);
        assert!(state.messages[0].is_flagged);
        assert_eq!(state.selected_message_labels, vec![label("work")]);
        assert!(state.selected_message_labels_resource.is_ready());
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
            LoadedFullMessage {
                message: full_message("first", "inbox", true, false),
                labels: Vec::new(),
            },
        ));
        assert_eq!(state.selected_message.as_deref(), Some("second"));
        assert_eq!(
            state.selected_message_resource.current_request(),
            Some(current)
        );
        assert!(state.selected_message_resource.is_loading());
        assert!(state.selected_full_message.is_none());
    }

    #[test]
    fn quick_reply_request_defaults_to_selected_sender() {
        let mut state = NativeAppState::default();
        let mut summary = message("message", "inbox", true);
        summary.subject = Some("Status".to_string());
        summary.from_name = Some("Ada Lovelace".to_string());
        summary.from_email = Some("ada@example.test".to_string());
        summary.message_id = Some("<message@example.test>".to_string());
        state.selected_message = Some("message".to_string());
        state.messages = vec![summary];

        let request =
            build_quick_reply_request(&state, Vec::new(), "  Thanks  ".to_string()).unwrap();

        assert_eq!(request.account_id, "account");
        assert_eq!(request.to, vec!["ada@example.test"]);
        assert_eq!(request.subject, "Re: Status");
        assert_eq!(request.text_body.as_deref(), Some("Thanks"));
        assert_eq!(
            request.in_reply_to.as_deref(),
            Some("<message@example.test>")
        );
    }

    #[test]
    fn forward_request_uses_explicit_recipient_and_omits_in_reply_to() {
        let mut state = NativeAppState::default();
        let mut full = full_message("message", "inbox", true, false);
        full.subject = Some("Plans".to_string());
        full.from_name = Some("Ada Lovelace".to_string());
        full.from_email = Some("ada@example.test".to_string());
        full.message_id = Some("<message@example.test>".to_string());
        state.selected_message = Some("message".to_string());
        state.selected_full_message = Some(full);
        state.compose.open = true;
        state.compose.mode = Some("forward".to_string());
        state.compose.account_id = Some("account".to_string());
        state.compose.subject = Some("Fwd: Plans".to_string());

        let request = build_quick_reply_request(
            &state,
            vec![
                " bob@example.test ".to_string(),
                "BOB@example.test".to_string(),
            ],
            "See below".to_string(),
        )
        .unwrap();

        assert_eq!(request.to, vec!["bob@example.test"]);
        assert_eq!(request.subject, "Fwd: Plans");
        assert_eq!(request.in_reply_to, None);
    }

    #[test]
    fn forward_request_requires_a_recipient() {
        let mut state = NativeAppState::default();
        state.selected_message = Some("message".to_string());
        state.selected_full_message = Some(full_message("message", "inbox", true, false));
        state.compose.mode = Some("forward".to_string());

        let err = build_quick_reply_request(&state, Vec::new(), "Body".to_string()).unwrap_err();

        assert_eq!(err, "Add at least one recipient.");
    }

    #[test]
    fn compose_reply_request_appends_quote_and_signature() {
        let mut state = NativeAppState::default();
        let mut full = full_message("message", "inbox", true, false);
        full.subject = Some("Status".to_string());
        full.from_name = Some("Ada Lovelace".to_string());
        full.from_email = Some("ada@example.test".to_string());
        full.message_id = Some("<message@example.test>".to_string());
        full.body = Some(MessageBody {
            html_body: None,
            text_body: Some("Original line\nSecond line".to_string()),
        });
        state.selected_message = Some("message".to_string());
        state.selected_full_message = Some(full);
        state.compose.open = true;
        state.compose.mode = Some("reply".to_string());
        state.compose.account_id = Some("account".to_string());
        state.compose.subject = Some("Re: Status".to_string());
        state.compose.in_reply_to = Some("<message@example.test>".to_string());
        state.compose.quoted_text = Some("Original line\nSecond line".to_string());
        state.compose.quoted_from = Some("Ada Lovelace".to_string());
        state.compose.signature = Some("Regards,\nAda".to_string());

        let request = build_compose_request(
            &state,
            vec![" ada@example.test ".to_string()],
            vec![" team@example.test ".to_string()],
            Vec::new(),
            " Re: Status ".to_string(),
            "  Thanks  ".to_string(),
        )
        .unwrap();

        assert_eq!(request.to, vec!["ada@example.test"]);
        assert_eq!(request.cc, vec!["team@example.test"]);
        assert_eq!(request.subject, "Re: Status");
        assert_eq!(
            request.text_body.as_deref(),
            Some(
                "Thanks\n\nAda Lovelace wrote:\n> Original line\n> Second line\n\n-- \nRegards,\nAda"
            )
        );
        assert_eq!(
            request.in_reply_to.as_deref(),
            Some("<message@example.test>")
        );
    }

    #[test]
    fn compose_forward_request_omits_in_reply_to_and_normalizes_recipients() {
        let mut state = NativeAppState::default();
        state.compose.open = true;
        state.compose.mode = Some("forward".to_string());
        state.compose.account_id = Some("account".to_string());
        state.compose.subject = Some("Fwd: Plans".to_string());
        state.compose.in_reply_to = Some("<ignored@example.test>".to_string());

        let request = build_compose_request(
            &state,
            vec![
                " bob@example.test ".to_string(),
                "BOB@example.test".to_string(),
            ],
            vec![" cc@example.test ".to_string()],
            vec![" audit@example.test ".to_string()],
            "Fwd: Plans".to_string(),
            "See below".to_string(),
        )
        .unwrap();

        assert_eq!(request.to, vec!["bob@example.test"]);
        assert_eq!(request.cc, vec!["cc@example.test"]);
        assert_eq!(request.bcc, vec!["audit@example.test"]);
        assert_eq!(request.in_reply_to, None);
        assert_eq!(request.text_body.as_deref(), Some("See below"));
    }

    #[test]
    fn compose_request_waits_for_signature_load() {
        let mut state = NativeAppState::default();
        state.compose.open = true;
        state.compose.mode = Some("compose".to_string());
        state.compose.account_id = Some("account".to_string());
        state.compose.signature_loading = true;

        let err = build_compose_request(
            &state,
            vec!["bob@example.test".to_string()],
            Vec::new(),
            Vec::new(),
            "Subject".to_string(),
            "Body".to_string(),
        )
        .unwrap_err();

        assert_eq!(err, "Wait for the account signature to load.");
    }

    #[test]
    fn settings_update_request_trims_name_and_sets_optional_values() {
        let request = build_account_settings_update_request(
            "  Ada  ".to_string(),
            "30".to_string(),
            "Regards,\nAda".to_string(),
        )
        .unwrap();

        assert_eq!(request.name.as_deref(), Some("Ada"));
        assert_eq!(request.sync_days_limit, Some(Some(30)));
        assert_eq!(request.signature, Some(Some("Regards,\nAda".to_string())));
    }

    #[test]
    fn settings_update_request_clears_blank_optional_values() {
        let request = build_account_settings_update_request(
            "Ada".to_string(),
            " ".to_string(),
            "\n  ".to_string(),
        )
        .unwrap();

        assert_eq!(request.sync_days_limit, Some(None));
        assert_eq!(request.signature, Some(None));
    }

    #[test]
    fn settings_update_request_rejects_invalid_values() {
        let err = build_account_settings_update_request(
            " ".to_string(),
            "30".to_string(),
            "Sig".to_string(),
        )
        .unwrap_err();
        assert_eq!(err, "Enter a display name.");

        let err = build_account_settings_update_request(
            "Ada".to_string(),
            "0".to_string(),
            "Sig".to_string(),
        )
        .unwrap_err();
        assert_eq!(err, "Download days must be a positive whole number.");
    }

    fn account_setup_draft() -> AccountSetupFormDraft {
        AccountSetupFormDraft {
            name: "  Ada  ".to_string(),
            email: "  ada@example.test  ".to_string(),
            imap_host: "  imap.example.test  ".to_string(),
            imap_port: "993".to_string(),
            imap_password: "imap-secret".to_string(),
            smtp_host: "  smtp.example.test  ".to_string(),
            smtp_port: "587".to_string(),
            smtp_password: "smtp-secret".to_string(),
            same_password: true,
        }
    }

    #[test]
    fn manual_account_request_trims_fields_and_reuses_imap_password() {
        let request = build_manual_account_request(account_setup_draft()).unwrap();

        assert_eq!(request.name, "Ada");
        assert_eq!(request.email, "ada@example.test");
        assert_eq!(request.provider_type, "imap");
        assert_eq!(request.auth_type, "basic");
        assert_eq!(request.host.as_deref(), Some("imap.example.test"));
        assert_eq!(request.port, Some(993));
        assert_eq!(request.password.as_deref(), Some("imap-secret"));
        assert_eq!(request.smtp_host.as_deref(), Some("smtp.example.test"));
        assert_eq!(request.smtp_port, Some(587));
        assert_eq!(request.smtp_password.as_deref(), Some("imap-secret"));
        assert_eq!(request.use_ssl, None);
    }

    #[test]
    fn manual_account_request_uses_distinct_smtp_password() {
        let mut draft = account_setup_draft();
        draft.same_password = false;

        let request = build_manual_account_request(draft).unwrap();

        assert_eq!(request.password.as_deref(), Some("imap-secret"));
        assert_eq!(request.smtp_password.as_deref(), Some("smtp-secret"));
    }

    #[test]
    fn microsoft_app_password_request_maps_defaults() {
        let mut draft = account_setup_draft();
        draft.imap_host = "outlook.office365.com".to_string();
        draft.smtp_host = "smtp.office365.com".to_string();
        draft.imap_password = "app-secret".to_string();

        let request = build_microsoft_app_password_request(draft).unwrap();

        assert_eq!(request.provider_type, "microsoft365");
        assert_eq!(request.auth_type, "app_password");
        assert_eq!(request.host.as_deref(), Some("outlook.office365.com"));
        assert_eq!(request.port, Some(993));
        assert_eq!(request.password.as_deref(), Some("app-secret"));
        assert_eq!(request.smtp_host.as_deref(), Some("smtp.office365.com"));
        assert_eq!(request.smtp_port, Some(587));
        assert_eq!(request.smtp_password.as_deref(), Some("app-secret"));
    }

    #[test]
    fn account_setup_request_rejects_invalid_values() {
        let mut draft = account_setup_draft();
        draft.email = "ada".to_string();
        let err = build_manual_account_request(draft).unwrap_err();
        assert_eq!(err, "Enter a valid email address.");

        let mut draft = account_setup_draft();
        draft.imap_port = "0".to_string();
        let err = build_manual_account_request(draft).unwrap_err();
        assert_eq!(err, "IMAP port must be between 1 and 65535.");

        let mut draft = account_setup_draft();
        draft.same_password = false;
        draft.smtp_password = " ".to_string();
        let err = build_manual_account_request(draft).unwrap_err();
        assert_eq!(err, "Enter a SMTP password.");
    }

    #[test]
    fn account_create_reload_closes_setup_on_first_run() {
        let mut state = NativeAppState {
            theme: ThemeMode::Light,
            density: DensityTokens::new(6),
            modals: ModalStates {
                account_setup_open: true,
                settings_open: true,
                advanced_search_open: true,
            },
            palette: PaletteState {
                open: true,
                query: "account".to_string(),
            },
            ..NativeAppState::default()
        };
        let loaded = NativeAppState {
            api_base_url: "http://localhost:3000".to_string(),
            accounts: vec![account("new")],
            accounts_resource: AsyncResource::ready(vec![account("new")]),
            selected_folder: Some("inbox".to_string()),
            selected_message: Some("message".to_string()),
            messages: vec![message("message", "inbox", true)],
            messages_resource: AsyncResource::ready(vec![message("message", "inbox", true)]),
            theme: ThemeMode::Dark,
            density: DensityTokens::new(1),
            ..NativeAppState::default()
        };

        apply_account_create_reload(&mut state, loaded);

        assert_eq!(state.accounts, vec![account("new")]);
        assert_eq!(state.theme, ThemeMode::Light);
        assert_eq!(state.density.level, 6);
        assert_eq!(state.selected_view, SelectedView::Mail);
        assert_eq!(state.selected_folder.as_deref(), Some("inbox"));
        assert_eq!(state.selected_message.as_deref(), Some("message"));
        assert!(!state.modals.account_setup_open);
        assert!(!state.modals.settings_open);
        assert!(!state.modals.advanced_search_open);
        assert!(state.palette.open);
        assert!(state.last_error.is_none());
    }

    #[test]
    fn account_create_reload_preserves_existing_workspace_selection() {
        let mut state = NativeAppState {
            accounts: vec![account("old")],
            accounts_resource: AsyncResource::ready(vec![account("old")]),
            selected_view: SelectedView::Calendar,
            selected_folder: Some("old-inbox".to_string()),
            selected_label: Some("label-a".to_string()),
            selected_message: Some("old-message".to_string()),
            selected_full_message: Some(full_message("old-message", "old-inbox", true, false)),
            selected_event: Some("event-1".to_string()),
            search: SearchState {
                query: "status".to_string(),
                submitted_query: Some("status".to_string()),
                ..SearchState::default()
            },
            messages: vec![message("old-message", "old-inbox", true)],
            messages_resource: AsyncResource::ready(vec![message(
                "old-message",
                "old-inbox",
                true,
            )]),
            compose: ComposeState {
                open: true,
                account_id: Some("old".to_string()),
                ..ComposeState::default()
            },
            modals: ModalStates {
                account_setup_open: true,
                settings_open: false,
                advanced_search_open: true,
            },
            last_error: Some("old".to_string()),
            ..NativeAppState::default()
        };
        let loaded = NativeAppState {
            api_base_url: "http://localhost:3000".to_string(),
            accounts: vec![account("old"), account("new")],
            accounts_resource: AsyncResource::ready(vec![account("old"), account("new")]),
            selected_view: SelectedView::Mail,
            selected_folder: Some("new-inbox".to_string()),
            selected_message: Some("new-message".to_string()),
            messages: vec![message("new-message", "new-inbox", true)],
            messages_resource: AsyncResource::ready(vec![message(
                "new-message",
                "new-inbox",
                true,
            )]),
            ..NativeAppState::default()
        };

        apply_account_create_reload(&mut state, loaded);

        assert_eq!(state.accounts, vec![account("old"), account("new")]);
        assert_eq!(state.selected_view, SelectedView::Calendar);
        assert_eq!(state.selected_folder.as_deref(), Some("old-inbox"));
        assert_eq!(state.selected_label.as_deref(), Some("label-a"));
        assert_eq!(state.selected_message.as_deref(), Some("old-message"));
        assert_eq!(state.selected_event.as_deref(), Some("event-1"));
        assert_eq!(state.search.submitted_query.as_deref(), Some("status"));
        assert_eq!(
            state.messages,
            vec![message("old-message", "old-inbox", true)]
        );
        assert!(state.compose.open);
        assert!(!state.modals.account_setup_open);
        assert!(!state.modals.advanced_search_open);
        assert!(state.last_error.is_none());
    }

    #[test]
    fn account_delete_reload_preserves_native_state_and_resets_deleted_compose() {
        let mut state = NativeAppState {
            theme: ThemeMode::Light,
            density: DensityTokens::new(7),
            selected_view: SelectedView::Calendar,
            selected_event: Some("event-1".to_string()),
            compose: ComposeState {
                open: true,
                account_id: Some("deleted".to_string()),
                revision: 4,
                ..ComposeState::default()
            },
            modals: ModalStates {
                account_setup_open: true,
                settings_open: true,
                advanced_search_open: true,
            },
            palette: PaletteState {
                open: true,
                query: "settings".to_string(),
            },
            sync: NativeSyncStatus {
                connected: true,
                ..NativeSyncStatus::default()
            },
            last_error: Some("old".to_string()),
            ..NativeAppState::default()
        };
        let loaded = NativeAppState {
            api_base_url: "http://localhost:3000".to_string(),
            accounts: vec![account("remaining")],
            accounts_resource: AsyncResource::ready(vec![account("remaining")]),
            theme: ThemeMode::Dark,
            density: DensityTokens::new(1),
            selected_view: SelectedView::Mail,
            ..NativeAppState::default()
        };

        apply_account_delete_reload(&mut state, "deleted", loaded);

        assert_eq!(state.accounts, vec![account("remaining")]);
        assert_eq!(state.theme, ThemeMode::Light);
        assert_eq!(state.density.level, 7);
        assert_eq!(state.selected_view, SelectedView::Calendar);
        assert_eq!(state.selected_event.as_deref(), Some("event-1"));
        assert!(state.modals.account_setup_open);
        assert!(!state.modals.settings_open);
        assert!(!state.modals.advanced_search_open);
        assert!(state.palette.open);
        assert!(state.sync.connected);
        assert!(!state.compose.open);
        assert_eq!(state.compose.revision, 5);
        assert!(state.last_error.is_none());
    }

    #[test]
    fn webhook_create_request_trims_fields_and_defaults_events() {
        let request = build_create_webhook_request(
            "  https://example.test/hook  ".to_string(),
            "  ".to_string(),
            "".to_string(),
        )
        .unwrap();

        assert_eq!(request.url, "https://example.test/hook");
        assert_eq!(request.secret, None);
        assert_eq!(request.events.as_deref(), Some("new_message"));
        assert_eq!(request.account_id, None);
    }

    #[test]
    fn webhook_create_request_accepts_secret_and_all_events() {
        let request = build_create_webhook_request(
            "https://example.test/hook".to_string(),
            "  secret  ".to_string(),
            "all".to_string(),
        )
        .unwrap();

        assert_eq!(request.secret.as_deref(), Some("secret"));
        assert_eq!(request.events.as_deref(), Some("all"));
    }

    #[test]
    fn webhook_create_request_rejects_invalid_values() {
        let err = build_create_webhook_request(
            " ".to_string(),
            "".to_string(),
            "new_message".to_string(),
        )
        .unwrap_err();
        assert_eq!(err, "Enter a webhook URL.");

        let err = build_create_webhook_request(
            "https://example.test/hook".to_string(),
            "".to_string(),
            "message_deleted".to_string(),
        )
        .unwrap_err();
        assert_eq!(err, "Choose a supported webhook event set.");
    }

    #[test]
    fn rule_save_request_builds_new_rule_payload() {
        let request = build_settings_rule_save_request(
            Some(SettingsRuleEditorMode::New),
            Some(" account ".to_string()),
            "  Move Ada reports  ".to_string(),
            MatchMode::Any,
            vec![
                Condition {
                    id: "condition-1".to_string(),
                    field: ConditionField::Subject,
                    operator: ConditionOperator::StartsWith,
                    value: " Report ".to_string(),
                },
                Condition {
                    id: "condition-2".to_string(),
                    field: ConditionField::IsFlagged,
                    operator: ConditionOperator::Contains,
                    value: "".to_string(),
                },
            ],
            vec![SettingsRuleActionDraft {
                action_type: "move_to_folder".to_string(),
                action_value: Some(" archive ".to_string()),
            }],
        )
        .unwrap();

        assert_eq!(request.mode, SettingsRuleEditorMode::New);
        assert_eq!(request.account_id, "account");
        assert_eq!(request.name, "Move Ada reports");
        assert_eq!(request.match_mode, "any");
        assert_eq!(
            request.conditions,
            vec![
                ConditionInput {
                    field: "subject".to_string(),
                    operator: "starts_with".to_string(),
                    value: "Report".to_string(),
                },
                ConditionInput {
                    field: "is_flagged".to_string(),
                    operator: "contains".to_string(),
                    value: "true".to_string(),
                },
            ]
        );
        assert_eq!(
            request.actions,
            vec![ActionInput {
                action_type: "move_to_folder".to_string(),
                action_value: Some("archive".to_string()),
            }]
        );
    }

    #[test]
    fn rule_save_request_builds_edit_payload_and_clears_non_move_values() {
        let request = build_settings_rule_save_request(
            Some(SettingsRuleEditorMode::Edit("rule-1".to_string())),
            Some("account".to_string()),
            "Mark read".to_string(),
            MatchMode::All,
            vec![Condition {
                id: "condition-1".to_string(),
                field: ConditionField::From,
                operator: ConditionOperator::Contains,
                value: "ada@example.test".to_string(),
            }],
            vec![SettingsRuleActionDraft {
                action_type: "mark_read".to_string(),
                action_value: Some("ignored-folder".to_string()),
            }],
        )
        .unwrap();

        assert_eq!(
            request.mode,
            SettingsRuleEditorMode::Edit("rule-1".to_string())
        );
        assert_eq!(request.match_mode, "all");
        assert_eq!(
            request.actions,
            vec![ActionInput {
                action_type: "mark_read".to_string(),
                action_value: None,
            }]
        );
    }

    #[test]
    fn rule_save_request_rejects_invalid_values() {
        let err = build_settings_rule_save_request(
            Some(SettingsRuleEditorMode::New),
            Some("account".to_string()),
            " ".to_string(),
            MatchMode::All,
            vec![new_advanced_condition("condition-1".to_string())],
            vec![SettingsRuleActionDraft {
                action_type: "mark_read".to_string(),
                action_value: None,
            }],
        )
        .unwrap_err();
        assert_eq!(err, "Enter a rule name.");

        let err = build_settings_rule_save_request(
            Some(SettingsRuleEditorMode::New),
            Some("account".to_string()),
            "Move".to_string(),
            MatchMode::All,
            vec![new_advanced_condition("condition-1".to_string())],
            vec![SettingsRuleActionDraft {
                action_type: "move_to_folder".to_string(),
                action_value: None,
            }],
        )
        .unwrap_err();
        assert_eq!(err, "Choose a folder for move actions.");

        let err = build_settings_rule_save_request(
            Some(SettingsRuleEditorMode::New),
            Some("account".to_string()),
            "Unsupported".to_string(),
            MatchMode::All,
            vec![new_advanced_condition("condition-1".to_string())],
            vec![SettingsRuleActionDraft {
                action_type: "bounce".to_string(),
                action_value: None,
            }],
        )
        .unwrap_err();
        assert_eq!(err, "Choose a supported rule action.");
    }

    #[test]
    fn rule_counts_text_pluralizes_counts() {
        let mut rule = RuleDto {
            id: "rule".to_string(),
            account_id: "account".to_string(),
            name: "Rule".to_string(),
            is_active: true,
            match_mode: "all".to_string(),
            priority: 0,
            conditions: vec![RuleConditionDto {
                id: "condition".to_string(),
                field: "from".to_string(),
                operator: "contains".to_string(),
                value: "ada".to_string(),
            }],
            actions: vec![RuleActionDto {
                id: "action".to_string(),
                action_type: "mark_read".to_string(),
                action_value: None,
            }],
        };
        assert_eq!(rule_counts_text(&rule), "1 condition, 1 action");

        rule.conditions.push(RuleConditionDto {
            id: "condition-2".to_string(),
            field: "subject".to_string(),
            operator: "contains".to_string(),
            value: "status".to_string(),
        });
        rule.actions.clear();
        assert_eq!(rule_counts_text(&rule), "2 conditions, 0 actions");
    }
}

fn palette_entries(
    state: &NativeAppState,
    state_entity: &Entity<NativeAppState>,
) -> Vec<PaletteEntry> {
    let mut entries = vec![
        PaletteEntry::new("compose", "New Message")
            .group("Actions")
            .hint("c")
            .subtitle("Compose a new email")
            .keyword("compose")
            .keyword("write")
            .disabled(state.accounts.is_empty())
            .action(|_, cx| cx.dispatch_action(&StartCompose)),
        PaletteEntry::new("advanced-search", "Advanced Search")
            .group("Actions")
            .subtitle("Build a structured mail search")
            .keyword("search")
            .keyword("filter")
            .action(|_, cx| cx.dispatch_action(&OpenAdvancedSearch)),
        PaletteEntry::new("settings", "Open Settings")
            .group("Actions")
            .subtitle("Open app and account settings")
            .keyword("preferences")
            .keyword("options")
            .action(|_, cx| cx.dispatch_action(&OpenSettings)),
        PaletteEntry::new("add-account", "Add Account")
            .group("Actions")
            .subtitle("Connect Gmail, Microsoft 365, or IMAP")
            .keyword("setup")
            .keyword("oauth")
            .keyword("imap")
            .action(|_, cx| cx.dispatch_action(&OpenAccountSetup)),
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

    for (kind, label) in [
        ("all", "All Inboxes"),
        ("unread", "Unread"),
        ("flagged", "Flagged"),
        ("snoozed", "Snoozed"),
    ] {
        let nav_state = state_entity.clone();
        let kind = kind.to_string();
        entries.push(
            PaletteEntry::new(format!("smart:{kind}"), label)
                .group("Navigate")
                .hint("smart")
                .subtitle("Open smart folder")
                .keyword("folder")
                .action(move |_, cx| {
                    start_message_collection_load(
                        &nav_state,
                        NativeMessageSource::SmartFolder(kind.clone()),
                        cx,
                    );
                }),
        );
    }

    if state.selected_message.is_some() {
        entries.push(
            PaletteEntry::new("message-reply", "Reply")
                .group("Message")
                .hint("r")
                .keyword("respond")
                .action(|_, cx| cx.dispatch_action(&StartReplyCompose)),
        );
        entries.push(
            PaletteEntry::new("message-forward", "Forward")
                .group("Message")
                .hint("f")
                .keyword("forward")
                .action(|_, cx| cx.dispatch_action(&StartForwardCompose)),
        );
    }

    if let Some((message_id, is_read, is_flagged)) = selected_palette_message_state(state) {
        let archive_state = state_entity.clone();
        let archive_id = message_id.clone();
        entries.push(
            PaletteEntry::new("message-archive", "Archive Message")
                .group("Message")
                .hint("e")
                .keyword("archive")
                .action(move |_, cx| {
                    start_message_mutation_and_refresh(
                        &archive_state,
                        NativeMessageMutation::Archive {
                            message_id: archive_id.clone(),
                        },
                        cx,
                    );
                }),
        );

        let delete_state = state_entity.clone();
        let delete_id = message_id.clone();
        entries.push(
            PaletteEntry::new("message-delete", "Delete Message")
                .group("Message")
                .hint("Del")
                .keyword("trash")
                .keyword("delete")
                .action(move |_, cx| {
                    start_message_mutation_and_refresh(
                        &delete_state,
                        NativeMessageMutation::Delete {
                            message_id: delete_id.clone(),
                        },
                        cx,
                    );
                }),
        );

        let read_state = state_entity.clone();
        let read_id = message_id.clone();
        entries.push(
            PaletteEntry::new(
                "message-toggle-read",
                if is_read { "Mark Unread" } else { "Mark Read" },
            )
            .group("Message")
            .hint("u")
            .keyword("read")
            .keyword("unread")
            .action(move |_, cx| {
                start_message_mutation_and_refresh(
                    &read_state,
                    NativeMessageMutation::SetRead {
                        message_id: read_id.clone(),
                        is_read: !is_read,
                    },
                    cx,
                );
            }),
        );

        let flag_state = state_entity.clone();
        let flag_id = message_id.clone();
        entries.push(
            PaletteEntry::new(
                "message-toggle-flag",
                if is_flagged {
                    "Unflag Message"
                } else {
                    "Flag Message"
                },
            )
            .group("Message")
            .keyword("flag")
            .keyword("star")
            .action(move |_, cx| {
                start_message_mutation_and_refresh(
                    &flag_state,
                    NativeMessageMutation::SetFlagged {
                        message_id: flag_id.clone(),
                        is_flagged: !is_flagged,
                    },
                    cx,
                );
            }),
        );
    }

    entries.extend(state.labels.iter().map(|label| {
        let nav_state = state_entity.clone();
        let label_id = label.id.clone();
        PaletteEntry::new(format!("label:{}", label.id), label.name.clone())
            .group("Labels")
            .hint("label")
            .subtitle("Open label")
            .keyword("label")
            .keyword("tag")
            .action(move |_, cx| {
                start_message_collection_load(
                    &nav_state,
                    NativeMessageSource::Label(label_id.clone()),
                    cx,
                );
            })
    }));

    entries.extend(state.folders_by_account.iter().flat_map(|(_, folders)| {
        folders
            .iter()
            .filter(|folder| !folder.is_excluded)
            .map(|folder| {
                let nav_state = state_entity.clone();
                let folder_id = folder.id.clone();
                PaletteEntry::new(format!("folder:{}", folder.id), folder.name.clone())
                    .group("Folders")
                    .hint(folder.full_path.clone())
                    .subtitle("Open folder")
                    .keyword("folder")
                    .keyword(folder.full_path.clone())
                    .action(move |_, cx| {
                        start_message_collection_load(
                            &nav_state,
                            NativeMessageSource::Folder(folder_id.clone()),
                            cx,
                        );
                    })
            })
    }));

    entries.extend(state.accounts.iter().map(|account| {
        let nav_state = state_entity.clone();
        let folder_id = preferred_account_folder_id(state, &account.id);
        let mut entry = PaletteEntry::new(account.id.clone(), account.name.clone())
            .group("Accounts")
            .subtitle(account.email.clone())
            .keyword("account")
            .keyword("inbox")
            .disabled(folder_id.is_none());
        if let Some(folder_id) = folder_id {
            entry = entry.action(move |_, cx| {
                start_message_collection_load(
                    &nav_state,
                    NativeMessageSource::Folder(folder_id.clone()),
                    cx,
                );
            });
        }
        entry
    }));

    entries
}

fn selected_palette_message_state(state: &NativeAppState) -> Option<(String, bool, bool)> {
    let selected_id = state.selected_message.as_ref()?;
    if let Some(summary) = selected_summary(state) {
        return Some((summary.id.clone(), summary.is_read, summary.is_flagged));
    }

    state
        .selected_full_message
        .as_ref()
        .filter(|message| message.id == *selected_id)
        .map(|message| (message.id.clone(), message.is_read, message.is_flagged))
}

fn preferred_account_folder_id(state: &NativeAppState, account_id: &str) -> Option<String> {
    let folders = state.folders_by_account.get(account_id)?;
    folders
        .iter()
        .find(|folder| folder.special_use.as_deref() == Some("inbox") && !folder.is_excluded)
        .or_else(|| folders.iter().find(|folder| !folder.is_excluded))
        .map(|folder| folder.id.clone())
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

fn sync_searchbox_value(
    searchbox: &Entity<Searchbox>,
    query: &str,
    cx: &mut Context<SidebarPlaceholder>,
) {
    if searchbox.read(cx).text() != query {
        searchbox.update(cx, |searchbox, cx| {
            searchbox.set_value(query.to_string(), cx);
        });
    }
}

fn open_advanced_search_modal(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AdvancedSearchModal>,
    app_cx: &mut App,
) {
    let group = state_entity.update(app_cx, |state, cx| {
        state.modals.advanced_search_open = true;
        cx.notify();
        state.search.condition_group.clone()
    });
    let _ = modal_entity.update(app_cx, |modal, cx| {
        modal.reset_to_group(group, cx);
    });
}

fn close_advanced_search_modal(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        state.modals.advanced_search_open = false;
        cx.notify();
    });
}

fn handle_sidebar_search_change(
    state_entity: &Entity<NativeAppState>,
    query: String,
    app_cx: &mut App,
) {
    let restore_source = state_entity.update(app_cx, |state, cx| {
        state.search.query = query.clone();
        if !query.trim().is_empty() {
            state.search.condition_group = None;
        }
        let restore_source = if query.trim().is_empty() && state.search.submitted_query.is_some() {
            let restore_source = state.search.previous_source.clone();
            if restore_source.is_none() {
                state.search.submitted_query = None;
                state.search.previous_source = None;
                state.selected_message = None;
                state.selected_message_ids.clear();
                state.selected_full_message = None;
                state.selected_message_resource.reset();
                state.selected_message_labels.clear();
                state.selected_message_labels_resource.reset();
                state.messages.clear();
                state.messages_resource.reset();
            }
            restore_source
        } else {
            None
        };
        cx.notify();
        restore_source
    });

    if let Some(source) = restore_source {
        start_message_collection_load(state_entity, source, app_cx);
    }
}

fn submit_sidebar_search(state_entity: &Entity<NativeAppState>, query: String, app_cx: &mut App) {
    let query = query.trim().to_string();
    if query.is_empty() {
        handle_sidebar_search_change(state_entity, query, app_cx);
        return;
    }

    start_message_collection_load(state_entity, NativeMessageSource::Search(query), app_cx);
}

fn start_advanced_search(
    state_entity: &Entity<NativeAppState>,
    group: ConditionGroup,
    app_cx: &mut App,
) {
    if !condition_group_has_active(&group) {
        return;
    }

    close_advanced_search_modal(state_entity, app_cx);
    start_message_collection_load(
        state_entity,
        NativeMessageSource::AdvancedSearch(group),
        app_cx,
    );
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

fn sidebar_footer(
    state_entity: &Entity<NativeAppState>,
    state: &NativeAppState,
    cx: &mut Context<SidebarPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let theme_is_dark = state.theme == ThemeMode::Dark;
    let theme_icon = if theme_is_dark {
        "icons/sun.svg"
    } else {
        "icons/moon.svg"
    };
    let theme_label = if theme_is_dark {
        "Light mode"
    } else {
        "Dark mode"
    };

    div()
        .px(px(8.0))
        .py(px(10.0))
        .border_t_1()
        .border_color(colors.stroke_neutral_subtle)
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    nav_item("Mail", state.selected_view == SelectedView::Mail, cx)
                        .id("nav-mail")
                        .cursor_pointer()
                        .on_click({
                            let state_entity = state_entity.clone();
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
                        let state_entity = state_entity.clone();
                        move |_, _, app_cx| {
                            state_entity.update(app_cx, |state, cx| {
                                state.selected_view = SelectedView::Calendar;
                                cx.notify();
                            });
                        }
                    }),
                ),
        )
        .child(
            div()
                .pt(px(8.0))
                .border_t_1()
                .border_color(colors.stroke_neutral_subtle)
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(typography.caption.size))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors.on_neutral)
                                .child("Density"),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(colors.on_subtle)
                                .child(format!("Level {}", state.density.level)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            IconButton::new("sidebar-density-down", "icons/chevron_down.svg")
                                .size(ButtonSize::Compact)
                                .disabled(state.density.level == 0)
                                .on_click(|_, _, app_cx| {
                                    app_cx.dispatch_action(&DecreaseDensity);
                                }),
                        )
                        .child(
                            IconButton::new("sidebar-density-up", "icons/chevron_up.svg")
                                .size(ButtonSize::Compact)
                                .disabled(state.density.level >= 8)
                                .on_click(|_, _, app_cx| {
                                    app_cx.dispatch_action(&IncreaseDensity);
                                }),
                        ),
                ),
        )
        .child(
            Button::new("sidebar-theme-toggle")
                .icon(theme_icon)
                .label(theme_label)
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .on_click(move |_, _, app_cx| {
                    if theme_is_dark {
                        app_cx.dispatch_action(&ApplyLightTheme);
                    } else {
                        app_cx.dispatch_action(&ApplyDarkTheme);
                    }
                }),
        )
        .child(
            Button::new("sidebar-settings")
                .label("Settings")
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .on_click(|_, _, app_cx| {
                    app_cx.dispatch_action(&OpenSettings);
                }),
        )
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
            sidebar_nav_row(
                label.to_string(),
                selected,
                count,
                0,
                smart_folder_icon(kind.as_str()),
                cx,
            )
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
            sidebar_nav_row(label.name.clone(), selected, None, 0, "icons/pin.svg", cx)
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
    icon: &'static str,
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
                .flex_1()
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
                .child(svg().path(icon).size(px(14.0)).text_color(if selected {
                    colors.accent
                } else {
                    colors.on_subtle
                }))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
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

fn folder_icon(folder: &Folder) -> &'static str {
    match folder.special_use.as_deref() {
        Some("trash") => "icons/delete.svg",
        Some("sent") | Some("drafts") => "icons/document.svg",
        Some("archive") => "icons/download.svg",
        Some("spam") => "icons/dismiss.svg",
        _ => "icons/folder.svg",
    }
}

fn smart_folder_icon(kind: &str) -> &'static str {
    match kind {
        "flagged" => "icons/pin.svg",
        "snoozed" => "icons/bell.svg",
        "unread" => "icons/document.svg",
        _ => "icons/list_tree.svg",
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

fn mail_message_list(
    state_entity: &Entity<NativeAppState>,
    state: &NativeAppState,
    cx: &mut Context<MessageListPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let row_height = state.density.message_row_height.max(66.0);
    let selected_id = state
        .selected_message
        .clone()
        .or_else(|| state.messages.first().map(|message| message.id.clone()));

    let mut root = div()
        .tab_index(0)
        .key_context("NativeMessageList")
        .flex()
        .flex_col()
        .w_full()
        .h_full()
        .min_h_0()
        .bg(colors.surface)
        .on_key_down({
            let ids = state
                .messages
                .iter()
                .map(|message| message.id.clone())
                .collect::<Vec<_>>();
            let selected_id = selected_id.clone();
            let state_entity = state_entity.clone();
            move |ev: &KeyDownEvent, _, app_cx| {
                if handle_message_list_key(
                    &state_entity,
                    &ids,
                    selected_id.as_deref(),
                    ev.keystroke.key.as_str(),
                    ev.keystroke.modifiers.control || ev.keystroke.modifiers.platform,
                    app_cx,
                ) {
                    app_cx.stop_propagation();
                    return;
                }

                if let Some(target) =
                    message_key_target(&ids, selected_id.as_deref(), ev.keystroke.key.as_str())
                {
                    start_full_message_load(&state_entity, &target, app_cx);
                }
            }
        });

    if let Some((message, error)) = message_list_status(state) {
        let color = if error {
            colors.status_error
        } else {
            colors.on_subtle_disabled
        };
        return root.child(
            div()
                .flex_1()
                .min_h(px(row_height * 2.0))
                .flex()
                .items_center()
                .justify_center()
                .px(px(18.0))
                .text_size(px(typography.body.size))
                .text_color(color)
                .child(message),
        );
    }

    let messages = state.messages.clone();
    let selected_id_for_rows = selected_id.clone();
    let selected_ids_for_rows = state
        .selected_message_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let row_state = state_entity.clone();
    let list_entity = cx.entity();
    root = root.child(
        uniform_list(
            "native-mail-message-rows",
            messages.len(),
            move |range, _, app_cx| {
                range
                    .map(|idx| {
                        mail_message_row(
                            idx,
                            &messages[idx],
                            selected_id_for_rows.as_deref() == Some(messages[idx].id.as_str()),
                            selected_ids_for_rows.contains(&messages[idx].id),
                            row_height,
                            row_state.clone(),
                            list_entity.clone(),
                            app_cx,
                        )
                    })
                    .collect::<Vec<_>>()
            },
        )
        .w_full()
        .h_full(),
    );

    root
}

fn message_list_status(state: &NativeAppState) -> Option<(String, bool)> {
    if state.messages_resource.is_loading() && state.messages.is_empty() {
        return Some(("Loading messages...".to_string(), false));
    }

    if let Some(error) = state.messages_resource.error() {
        return Some((format!("Messages failed to load: {error}"), true));
    }

    if !state.messages.is_empty() {
        return None;
    }

    if let Some(query) = state.search.submitted_query.as_deref() {
        return Some((format!("No results for \"{query}\""), false));
    }

    if state.search.condition_group.is_some() {
        return Some(("No advanced search results".to_string(), false));
    }

    if state.selected_folder.is_none() && state.selected_label.is_none() {
        return Some(("No folder selected".to_string(), false));
    }

    Some(("No messages in this view".to_string(), false))
}

fn message_key_target(ids: &[String], selected_id: Option<&str>, key: &str) -> Option<String> {
    if ids.is_empty() {
        return None;
    }

    let current = selected_id
        .and_then(|id| ids.iter().position(|message_id| message_id == id))
        .unwrap_or(0);
    let next = match key {
        "up" | "k" => current.saturating_sub(1),
        "down" | "j" => (current + 1).min(ids.len() - 1),
        "home" => 0,
        "end" => ids.len() - 1,
        "pageup" => current.saturating_sub(5),
        "pagedown" => (current + 5).min(ids.len() - 1),
        "return" | "space" => current,
        _ => return None,
    };

    Some(ids[next].clone())
}

fn handle_message_list_key(
    state_entity: &Entity<NativeAppState>,
    ids: &[String],
    selected_id: Option<&str>,
    key: &str,
    command: bool,
    app_cx: &mut App,
) -> bool {
    if ids.is_empty() {
        return false;
    }

    if command && key == "a" {
        state_entity.update(app_cx, |state, cx| {
            let all_selected = message_select_all_state(state) == CheckboxState::Checked;
            set_all_message_selection(state, !all_selected);
            cx.notify();
        });
        return true;
    }

    match key {
        "escape" => {
            state_entity.update(app_cx, |state, cx| {
                if state.selected_message_ids.is_empty() {
                    state.selected_message = None;
                    state.selected_full_message = None;
                    state.selected_message_resource.reset();
                    state.selected_message_labels.clear();
                    state.selected_message_labels_resource.reset();
                    reset_compose_state(state, None);
                } else {
                    state.selected_message_ids.clear();
                }
                cx.notify();
            });
            true
        }
        "x" => {
            if let Some(selected_id) = selected_id {
                state_entity.update(app_cx, |state, cx| {
                    toggle_message_selection(state, selected_id);
                    cx.notify();
                });
                true
            } else {
                false
            }
        }
        "e" => start_message_list_target_action(
            state_entity,
            selected_id,
            BulkAction::Archive,
            |message_id| NativeMessageMutation::Archive { message_id },
            app_cx,
        ),
        "backspace" | "delete" => start_message_list_target_action(
            state_entity,
            selected_id,
            BulkAction::Delete,
            |message_id| NativeMessageMutation::Delete { message_id },
            app_cx,
        ),
        "u" => start_message_list_read_toggle(state_entity, selected_id, app_cx),
        _ => false,
    }
}

fn start_message_list_target_action(
    state_entity: &Entity<NativeAppState>,
    selected_id: Option<&str>,
    bulk_action: BulkAction,
    single_mutation: impl Fn(String) -> NativeMessageMutation,
    app_cx: &mut App,
) -> bool {
    let state = state_entity.read(app_cx).clone();
    if state.selected_message_ids.is_empty() {
        let Some(message_id) = selected_id.map(str::to_string) else {
            return false;
        };
        start_message_mutation_and_refresh(state_entity, single_mutation(message_id), app_cx);
    } else {
        start_bulk_message_action(state_entity, bulk_action, app_cx);
    }
    true
}

fn start_message_list_read_toggle(
    state_entity: &Entity<NativeAppState>,
    selected_id: Option<&str>,
    app_cx: &mut App,
) -> bool {
    let state = state_entity.read(app_cx).clone();
    if state.selected_message_ids.is_empty() {
        let Some(message_id) = selected_id else {
            return false;
        };
        let is_read = state
            .messages
            .iter()
            .find(|message| message.id == message_id)
            .map(|message| message.is_read)
            .unwrap_or(true);
        start_message_mutation_and_refresh(
            state_entity,
            NativeMessageMutation::SetRead {
                message_id: message_id.to_string(),
                is_read: !is_read,
            },
            app_cx,
        );
    } else {
        let selected_ids = selected_action_message_ids(&state, None)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let any_unread = state
            .messages
            .iter()
            .filter(|message| selected_ids.contains(&message.id))
            .any(|message| !message.is_read);
        start_bulk_message_action(
            state_entity,
            if any_unread {
                BulkAction::MarkRead
            } else {
                BulkAction::MarkUnread
            },
            app_cx,
        );
    }
    true
}

fn message_row_context_menu(
    state_entity: Entity<NativeAppState>,
    message: Message,
    position: Point<Pixels>,
    app_cx: &mut App,
) -> Entity<ContextMenu> {
    let message_id = message.id.clone();
    let open_state = state_entity.clone();
    let reply_state = state_entity.clone();
    let forward_state = state_entity.clone();
    let archive_state = state_entity.clone();
    let delete_state = state_entity.clone();
    let read_state = state_entity.clone();
    let flag_state = state_entity;
    let is_read = message.is_read;
    let is_flagged = message.is_flagged;

    app_cx.new(move |_| {
        ContextMenu::build(position)
            .action("Open", {
                let message_id = message_id.clone();
                move |_, _, app_cx| {
                    start_full_message_load(&open_state, &message_id, app_cx);
                }
            })
            .action("Reply", {
                let message_id = message_id.clone();
                move |_, _, app_cx| {
                    start_full_message_load(&reply_state, &message_id, app_cx);
                    start_reply_draft(&reply_state, app_cx);
                }
            })
            .action("Forward", {
                let message_id = message_id.clone();
                move |_, _, app_cx| {
                    start_full_message_load(&forward_state, &message_id, app_cx);
                    start_forward_draft(&forward_state, app_cx);
                }
            })
            .separator()
            .action("Archive", {
                let message_id = message_id.clone();
                move |_, _, app_cx| {
                    start_message_mutation_and_refresh(
                        &archive_state,
                        NativeMessageMutation::Archive {
                            message_id: message_id.clone(),
                        },
                        app_cx,
                    );
                }
            })
            .action("Delete", {
                let message_id = message_id.clone();
                move |_, _, app_cx| {
                    start_message_mutation_and_refresh(
                        &delete_state,
                        NativeMessageMutation::Delete {
                            message_id: message_id.clone(),
                        },
                        app_cx,
                    );
                }
            })
            .separator()
            .action(if is_read { "Mark unread" } else { "Mark read" }, {
                let message_id = message_id.clone();
                move |_, _, app_cx| {
                    start_message_mutation_and_refresh(
                        &read_state,
                        NativeMessageMutation::SetRead {
                            message_id: message_id.clone(),
                            is_read: !is_read,
                        },
                        app_cx,
                    );
                }
            })
            .action(if is_flagged { "Unflag" } else { "Flag" }, {
                let message_id = message_id.clone();
                move |_, _, app_cx| {
                    start_message_mutation_and_refresh(
                        &flag_state,
                        NativeMessageMutation::SetFlagged {
                            message_id: message_id.clone(),
                            is_flagged: !is_flagged,
                        },
                        app_cx,
                    );
                }
            })
    })
}

fn mail_message_row(
    idx: usize,
    message: &Message,
    selected: bool,
    checked: bool,
    row_height: f32,
    state_entity: Entity<NativeAppState>,
    list_entity: Entity<MessageListPlaceholder>,
    cx: &mut App,
) -> impl IntoElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let subject = message
        .subject
        .clone()
        .filter(|subject| !subject.trim().is_empty())
        .unwrap_or_else(|| "(no subject)".to_string());
    let sender = message_sender(message);
    let preview = message.preview.clone().unwrap_or_default();
    let date = message
        .date
        .clone()
        .filter(|date| !date.trim().is_empty())
        .unwrap_or_else(|| "No date".to_string());
    let unread = !message.is_read;
    let message_id = message.id.clone();
    let checkbox_message_id = message.id.clone();
    let click_state = state_entity.clone();
    let checkbox_state = state_entity.clone();
    let context_message = message.clone();
    let context_state = state_entity.clone();
    let context_list = list_entity.clone();

    div()
        .id(("native-mail-message-row", idx as u64))
        .h(px(row_height))
        .w_full()
        .flex()
        .items_center()
        .bg(if selected {
            colors.neutral_selected
        } else {
            colors.surface
        })
        .border_b_1()
        .border_color(colors.stroke_neutral_subtle)
        .cursor_pointer()
        .hover({
            let hover = colors.subtle_hover;
            let selected_bg = colors.neutral_selected;
            move |style| style.bg(if selected { selected_bg } else { hover })
        })
        .on_click(move |_, _, app_cx| {
            start_full_message_load(&click_state, &message_id, app_cx);
        })
        .on_mouse_down(MouseButton::Right, move |ev, _, app_cx| {
            let menu = message_row_context_menu(
                context_state.clone(),
                context_message.clone(),
                ev.position,
                app_cx,
            );
            context_list.update(app_cx, |list, cx| {
                list.context_menu = Some(menu);
                cx.notify();
            });
            app_cx.stop_propagation();
        })
        .child(div().h_full().w(px(3.0)).bg(if selected || unread {
            colors.accent
        } else {
            colors.surface
        }))
        .child(
            div()
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(10.0))
                .min_w_0()
                .flex_1()
                .child(
                    Checkbox::new(("message-row-select", idx))
                        .checked(checked)
                        .on_click({
                            move |_, _, _, app_cx| {
                                app_cx.stop_propagation();
                                checkbox_state.update(app_cx, |state, cx| {
                                    toggle_message_selection(state, &checkbox_message_id);
                                    cx.notify();
                                });
                            }
                        }),
                )
                .child(
                    div()
                        .flex_none()
                        .size(px(30.0))
                        .rounded(px(999.0))
                        .bg(if unread {
                            colors.accent
                        } else {
                            colors.neutral
                        })
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if unread {
                            colors.on_accent
                        } else {
                            colors.on_subtle
                        })
                        .child(sender_initials(&sender)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_size(px(typography.body.size))
                                        .font_weight(if unread {
                                            FontWeight::SEMIBOLD
                                        } else {
                                            FontWeight::NORMAL
                                        })
                                        .text_color(colors.on_neutral)
                                        .child(sender),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .max_w(px(92.0))
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_size(px(typography.caption.size))
                                        .text_color(colors.on_subtle)
                                        .child(date),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_size(px(typography.body.size))
                                        .font_weight(if unread {
                                            FontWeight::SEMIBOLD
                                        } else {
                                            FontWeight::NORMAL
                                        })
                                        .text_color(colors.on_neutral)
                                        .child(subject),
                                )
                                .when(message.is_flagged, |row| {
                                    row.child(message_row_badge("Flagged", cx))
                                })
                                .when(message.has_attachments, |row| {
                                    row.child(message_row_badge("Attachment", cx))
                                })
                                .when(message.snoozed_until.is_some(), |row| {
                                    row.child(message_row_badge("Snoozed", cx))
                                }),
                        )
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(typography.caption.size))
                                .text_color(colors.on_subtle)
                                .child(preview),
                        ),
                ),
        )
}

fn message_row_badge(label: &'static str, cx: &mut App) -> gpui::Div {
    let theme = cx.theme();
    div()
        .flex_none()
        .px(px(5.0))
        .py(px(1.0))
        .rounded(px(3.0))
        .bg(theme.colors.neutral)
        .text_size(px(10.0))
        .text_color(theme.colors.on_subtle)
        .child(label)
}

fn message_sender(message: &Message) -> String {
    message
        .from_name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| message.from_email.clone())
        .unwrap_or_else(|| "Unknown sender".to_string())
}

fn sender_initials(sender: &str) -> String {
    let mut initials = sender
        .split_whitespace()
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect::<String>();
    if initials.is_empty() {
        initials = sender.chars().take(1).collect();
    }
    if initials.is_empty() {
        "U".to_string()
    } else {
        initials.to_uppercase()
    }
}

fn selected_regular_folder_id(state: &NativeAppState) -> Option<&str> {
    if state.search.submitted_query.is_some()
        || state.search.condition_group.is_some()
        || state.selected_label.is_some()
    {
        return None;
    }

    state
        .selected_folder
        .as_deref()
        .filter(|folder_id| !folder_id.starts_with("smart:"))
}

fn current_mailbox_title(state: &NativeAppState) -> String {
    if state.selected_view == SelectedView::Calendar {
        return "Calendar".to_string();
    }

    if let Some(query) = state.search.submitted_query.as_deref() {
        return format!("Results for \"{query}\"");
    }

    if state.search.condition_group.is_some() {
        return "Advanced search results".to_string();
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

fn reading_label_row(
    state_entity: &Entity<NativeAppState>,
    state: &NativeAppState,
    message_id: &str,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let selected_account_id = selected_account_id(state);
    let attached_ids = state
        .selected_message_labels
        .iter()
        .map(|label| label.id.as_str())
        .collect::<BTreeSet<_>>();
    let addable_labels = state
        .labels
        .iter()
        .filter(|label| {
            selected_account_id
                .as_deref()
                .map_or(true, |id| label.account_id == id)
        })
        .filter(|label| !attached_ids.contains(label.id.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    let mut row = div()
        .pt(px(10.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_wrap();

    if state.selected_message_labels.is_empty()
        && state.selected_message_labels_resource.is_loading()
    {
        row = row.child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(colors.on_subtle)
                .child("Loading labels..."),
        );
    }

    for (idx, label) in state.selected_message_labels.iter().enumerate() {
        let state_entity = state_entity.clone();
        let message_id = message_id.to_string();
        let label_id = label.id.clone();
        row = row.child(
            Button::new(("reading-remove-label", idx))
                .label(format!("{} x", label.name))
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .disabled(state.selected_message_labels_resource.is_loading())
                .on_click(move |_, _, app_cx| {
                    start_message_label_mutation(
                        &state_entity,
                        NativeLabelMutation::Remove {
                            message_id: message_id.clone(),
                            label_id: label_id.clone(),
                        },
                        app_cx,
                    );
                }),
        );
    }

    for (idx, label) in addable_labels.into_iter().enumerate() {
        let state_entity = state_entity.clone();
        let message_id = message_id.to_string();
        let label_id = label.id.clone();
        row = row.child(
            Button::new(("reading-add-label", idx))
                .label(format!("+ {}", label.name))
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .disabled(state.selected_message_labels_resource.is_loading())
                .on_click(move |_, _, app_cx| {
                    start_message_label_mutation(
                        &state_entity,
                        NativeLabelMutation::Add {
                            message_id: message_id.clone(),
                            label_id: label_id.clone(),
                        },
                        app_cx,
                    );
                }),
        );
    }

    if let Some(error) = state.selected_message_labels_resource.error() {
        row = row.child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(colors.status_error)
                .child(format!("Labels failed: {error}")),
        );
    }

    row
}

fn compose_placeholder(
    state_entity: &Entity<NativeAppState>,
    state: &NativeAppState,
    to_input: Entity<TokenInput>,
    cc_input: Entity<TokenInput>,
    bcc_input: Entity<TokenInput>,
    subject_input: Entity<TextInput>,
    body_input: Entity<Textarea>,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let compose = &state.compose;
    let mode = compose.mode.as_deref().unwrap_or("compose");
    let mode_label = compose_mode_label(mode);
    let header_subject = compose
        .subject
        .as_deref()
        .map(str::trim)
        .filter(|subject| !subject.is_empty())
        .unwrap_or("No subject")
        .to_string();
    let show_cc = compose.show_cc || !cc_input.read(cx).values().is_empty();
    let show_bcc = compose.show_bcc || !bcc_input.read(cx).values().is_empty();
    let has_to = token_input_has_value(&to_input, cx);
    let has_body = !body_input.read(cx).text().trim().is_empty();
    let can_send = !compose.sending && !compose.signature_loading && has_to && has_body;
    let compose_status = compose
        .error
        .clone()
        .map(|error| (error, true))
        .or_else(|| {
            compose
                .signature_loading
                .then(|| ("Loading signature...".to_string(), false))
        })
        .or_else(|| compose.notice.clone().map(|notice| (notice, false)));
    let signature = compose
        .signature
        .clone()
        .filter(|signature| !signature.trim().is_empty());
    let quoted_text = (mode == "reply")
        .then(|| compose.quoted_text.clone())
        .flatten()
        .filter(|quote| !quote.trim().is_empty());
    let quoted_from = compose.quoted_from.clone();
    let mut body_section = div()
        .id("compose-body-scroll")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(20.0))
        .py(px(16.0))
        .flex()
        .flex_col()
        .gap(px(14.0))
        .child(body_input.clone());
    if let Some(signature) = signature.as_deref() {
        body_section = body_section.child(compose_signature_block(signature, cx));
    }
    if let Some(quote) = quoted_text.as_deref() {
        body_section = body_section.child(compose_quote_block(quote, quoted_from.as_deref(), cx));
    }

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
                .py(px(12.0))
                .border_b_1()
                .border_color(colors.stroke_neutral_subtle)
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .h(px(24.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .rounded(px(4.0))
                        .bg(colors.neutral)
                        .text_size(px(typography.caption.size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.on_neutral)
                        .child(mode_label),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(typography.subtitle.size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.on_neutral)
                        .child(header_subject),
                )
                .child(
                    Button::new("compose-header-discard")
                        .label("Discard")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .disabled(compose.sending)
                        .on_click({
                            let state_entity = state_entity.clone();
                            move |_, _, app_cx| {
                                discard_compose(&state_entity, app_cx);
                            }
                        }),
                ),
        )
        .child(
            div()
                .px(px(20.0))
                .py(px(12.0))
                .border_b_1()
                .border_color(colors.stroke_neutral_subtle)
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(compose_field_label("To", cx))
                        .child(div().flex_1().min_w_0().child(to_input.clone()))
                        .when(!show_cc, |row| {
                            row.child(
                                Button::new("compose-show-cc")
                                    .label("Cc")
                                    .appearance(ButtonAppearance::Subtle)
                                    .size(ButtonSize::Compact)
                                    .on_click({
                                        let state_entity = state_entity.clone();
                                        move |_, _, app_cx| {
                                            set_compose_cc_visible(&state_entity, true, app_cx);
                                        }
                                    }),
                            )
                        })
                        .when(!show_bcc, |row| {
                            row.child(
                                Button::new("compose-show-bcc")
                                    .label("Bcc")
                                    .appearance(ButtonAppearance::Subtle)
                                    .size(ButtonSize::Compact)
                                    .on_click({
                                        let state_entity = state_entity.clone();
                                        move |_, _, app_cx| {
                                            set_compose_bcc_visible(&state_entity, true, app_cx);
                                        }
                                    }),
                            )
                        }),
                )
                .when(show_cc, |fields| {
                    fields.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(compose_field_label("Cc", cx))
                            .child(div().flex_1().min_w_0().child(cc_input.clone())),
                    )
                })
                .when(show_bcc, |fields| {
                    fields.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(compose_field_label("Bcc", cx))
                            .child(div().flex_1().min_w_0().child(bcc_input.clone())),
                    )
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(compose_field_label("Subject", cx))
                        .child(div().flex_1().min_w_0().child(subject_input.clone())),
                ),
        )
        .child(body_section)
        .child(
            div()
                .border_t_1()
                .border_color(colors.stroke_neutral_subtle)
                .px(px(20.0))
                .py(px(12.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    Button::new("compose-send")
                        .label(if compose.sending {
                            "Sending..."
                        } else {
                            "Send"
                        })
                        .appearance(ButtonAppearance::Accent)
                        .size(ButtonSize::Compact)
                        .disabled(!can_send)
                        .on_click({
                            let state_entity = state_entity.clone();
                            let to_input = to_input.clone();
                            let cc_input = cc_input.clone();
                            let bcc_input = bcc_input.clone();
                            let subject_input = subject_input.clone();
                            let body_input = body_input.clone();
                            move |_, _, app_cx| {
                                start_compose_send(
                                    &state_entity,
                                    to_input.clone(),
                                    cc_input.clone(),
                                    bcc_input.clone(),
                                    subject_input.clone(),
                                    body_input.clone(),
                                    app_cx,
                                );
                            }
                        }),
                )
                .child(
                    Button::new("compose-discard")
                        .label("Discard")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .disabled(compose.sending)
                        .on_click({
                            let state_entity = state_entity.clone();
                            move |_, _, app_cx| {
                                discard_compose(&state_entity, app_cx);
                            }
                        }),
                )
                .when_some(compose_status, |footer, (status, is_error)| {
                    footer.child(
                        div()
                            .text_size(px(typography.caption.size))
                            .text_color(if is_error {
                                colors.status_error
                            } else {
                                colors.on_subtle
                            })
                            .child(status),
                    )
                }),
        )
}

fn compose_mode_label(mode: &str) -> &'static str {
    match mode {
        "reply" => "Reply",
        "forward" => "Forward",
        _ => "New message",
    }
}

fn compose_field_label(label: &'static str, cx: &mut Context<ReadingPanePlaceholder>) -> gpui::Div {
    let theme = cx.theme();
    div()
        .w(px(58.0))
        .text_size(px(theme.typography.caption.size))
        .text_color(theme.colors.on_subtle)
        .child(label)
}

fn compose_signature_block(signature: &str, cx: &mut Context<ReadingPanePlaceholder>) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let mut lines = div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .text_size(px(typography.body.size))
        .line_height(px(typography.body.line_height))
        .text_color(colors.on_subtle);

    for line in signature.lines() {
        lines = lines.child(SharedString::from(if line.is_empty() {
            " ".to_string()
        } else {
            line.to_string()
        }));
    }

    div()
        .border_t_1()
        .border_color(colors.stroke_neutral_subtle)
        .pt(px(10.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(colors.on_subtle)
                .child("--"),
        )
        .child(lines)
}

fn compose_quote_block(
    quote: &str,
    from: Option<&str>,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let heading = from
        .map(str::trim)
        .filter(|from| !from.is_empty())
        .map(|from| format!("{from} wrote:"))
        .unwrap_or_else(|| "Original message:".to_string());
    let mut lines = div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .text_size(px(typography.caption.size))
        .line_height(px(typography.body.line_height))
        .text_color(colors.on_subtle);

    for line in quote.lines() {
        lines = lines.child(SharedString::from(if line.is_empty() {
            " ".to_string()
        } else {
            line.to_string()
        }));
    }

    div()
        .border_l_1()
        .border_color(colors.stroke_neutral)
        .pl(px(10.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(typography.caption.size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.on_subtle)
                .child(heading),
        )
        .child(lines)
}

fn token_input_has_value(input: &Entity<TokenInput>, cx: &App) -> bool {
    input
        .read(cx)
        .values()
        .iter()
        .any(|token| token.valid && !token.value.trim().is_empty())
}

fn set_compose_cc_visible(state_entity: &Entity<NativeAppState>, visible: bool, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        state.compose.show_cc = visible;
        cx.notify();
    });
}

fn set_compose_bcc_visible(state_entity: &Entity<NativeAppState>, visible: bool, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        state.compose.show_bcc = visible;
        cx.notify();
    });
}

fn reading_placeholder(
    state_entity: &Entity<NativeAppState>,
    state: &NativeAppState,
    recipient_input: Entity<TokenInput>,
    quick_reply_input: Entity<TextInput>,
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
    let is_read = state
        .selected_full_message
        .as_ref()
        .map(|message| message.is_read)
        .or_else(|| summary.map(|message| message.is_read))
        .unwrap_or(true);
    let message_id = selected_id.unwrap_or_default();
    let action_busy = state.messages_resource.is_loading() || state.compose.sending;
    let quick_reply_has_body = !quick_reply_input.read(cx).text().trim().is_empty();
    let quick_reply_has_recipient =
        !recipient_input.read(cx).values().is_empty() || selected_from_email(state).is_some();
    let can_send_quick_reply =
        !state.compose.sending && quick_reply_has_body && quick_reply_has_recipient;
    let compose_status = state
        .compose
        .error
        .clone()
        .map(|error| (error, true))
        .or_else(|| state.compose.notice.clone().map(|notice| (notice, false)));

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
                .child(reading_label_row(state_entity, state, &message_id, cx))
                .child(
                    div()
                        .pt(px(10.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            Button::new("reading-reply")
                                .label("Reply")
                                .appearance(ButtonAppearance::Neutral)
                                .size(ButtonSize::Compact)
                                .disabled(action_busy)
                                .on_click({
                                    let state_entity = state_entity.clone();
                                    move |_, _, app_cx| {
                                        start_reply_draft(&state_entity, app_cx);
                                    }
                                }),
                        )
                        .child(
                            Button::new("reading-forward")
                                .label("Forward")
                                .appearance(ButtonAppearance::Neutral)
                                .size(ButtonSize::Compact)
                                .disabled(action_busy)
                                .on_click({
                                    let state_entity = state_entity.clone();
                                    move |_, _, app_cx| {
                                        start_forward_draft(&state_entity, app_cx);
                                    }
                                }),
                        )
                        .child(
                            Button::new("reading-toggle-read")
                                .label(if is_read { "Mark unread" } else { "Mark read" })
                                .appearance(ButtonAppearance::Subtle)
                                .size(ButtonSize::Compact)
                                .disabled(action_busy)
                                .on_click({
                                    let state_entity = state_entity.clone();
                                    let message_id = message_id.clone();
                                    move |_, _, app_cx| {
                                        start_message_mutation_and_refresh(
                                            &state_entity,
                                            NativeMessageMutation::SetRead {
                                                message_id: message_id.clone(),
                                                is_read: !is_read,
                                            },
                                            app_cx,
                                        );
                                    }
                                }),
                        )
                        .child(
                            Button::new("reading-archive")
                                .label("Archive")
                                .appearance(ButtonAppearance::Subtle)
                                .size(ButtonSize::Compact)
                                .disabled(action_busy)
                                .on_click({
                                    let state_entity = state_entity.clone();
                                    let message_id = message_id.clone();
                                    move |_, _, app_cx| {
                                        start_message_mutation_and_refresh(
                                            &state_entity,
                                            NativeMessageMutation::Archive {
                                                message_id: message_id.clone(),
                                            },
                                            app_cx,
                                        );
                                    }
                                }),
                        )
                        .child(
                            Button::new("reading-delete")
                                .label("Delete")
                                .appearance(ButtonAppearance::Danger)
                                .size(ButtonSize::Compact)
                                .disabled(action_busy)
                                .on_click({
                                    let state_entity = state_entity.clone();
                                    let message_id = message_id.clone();
                                    move |_, _, app_cx| {
                                        start_message_mutation_and_refresh(
                                            &state_entity,
                                            NativeMessageMutation::Delete {
                                                message_id: message_id.clone(),
                                            },
                                            app_cx,
                                        );
                                    }
                                }),
                        )
                        .child(
                            Button::new("reading-snooze")
                                .label("Snooze")
                                .appearance(ButtonAppearance::Subtle)
                                .size(ButtonSize::Compact)
                                .disabled(action_busy)
                                .on_click({
                                    let state_entity = state_entity.clone();
                                    let snooze_picker = snooze_picker.clone();
                                    let message_id = message_id.clone();
                                    move |_, _, app_cx| {
                                        start_snooze_from_picker(
                                            &state_entity,
                                            snooze_picker.clone(),
                                            message_id.clone(),
                                            app_cx,
                                        );
                                    }
                                }),
                        )
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
        .child(
            div()
                .border_t_1()
                .border_color(colors.stroke_neutral_subtle)
                .px(px(20.0))
                .py(px(12.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .w(px(34.0))
                                .text_size(px(typography.caption.size))
                                .text_color(colors.on_subtle)
                                .child("To"),
                        )
                        .child(div().flex_1().min_w_0().child(recipient_input.clone())),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().flex_1().min_w_0().child(quick_reply_input.clone()))
                        .child(
                            Button::new("reading-send-quick-reply")
                                .label(if state.compose.sending {
                                    "Sending..."
                                } else {
                                    "Send"
                                })
                                .appearance(ButtonAppearance::Accent)
                                .size(ButtonSize::Compact)
                                .disabled(!can_send_quick_reply)
                                .on_click({
                                    let state_entity = state_entity.clone();
                                    let recipient_input = recipient_input.clone();
                                    let quick_reply_input = quick_reply_input.clone();
                                    move |_, _, app_cx| {
                                        start_quick_reply_send(
                                            &state_entity,
                                            recipient_input.clone(),
                                            quick_reply_input.clone(),
                                            app_cx,
                                        );
                                    }
                                }),
                        ),
                )
                .when_some(compose_status, |footer, (status, is_error)| {
                    footer.child(
                        div()
                            .text_size(px(typography.caption.size))
                            .text_color(if is_error {
                                colors.status_error
                            } else {
                                colors.on_subtle
                            })
                            .child(status),
                    )
                }),
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
