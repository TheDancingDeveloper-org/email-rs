use std::collections::BTreeMap;
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context as _, Result};
use email_client::EmailClient;
use email_server::{run_server, Config};
use email_types::{Account, Folder, Label, Message};
use fluent_app::FluentApp;
use fluent_core::{Theme, ThemeProvider as _};
use fluent_layout::{
    CommandPalette, DockPanel, DockPosition, ModalHost, PaletteEntry, Pane, PaneConstraints,
    PaneGroup, Workspace,
};
use gpui::{
    actions, div, prelude::*, px, Context, Entity, FontWeight, IntoElement, KeyBinding, Render,
    Window,
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

    FluentApp::new(APP_TITLE)
        .window_size(1280.0, 800.0)
        .dark_theme()
        .run(move |cx| {
            cx.bind_keys([
                KeyBinding::new("ctrl-k", TogglePalette, None),
                KeyBinding::new("ctrl-shift-l", ApplyLightTheme, None),
                KeyBinding::new("ctrl-shift-d", ApplyDarkTheme, None),
                KeyBinding::new("ctrl-plus", IncreaseDensity, None),
                KeyBinding::new("ctrl-minus", DecreaseDensity, None),
            ]);

            let modal_host = cx.new(ModalHost::new);
            let sidebar = cx.new({
                let state = initial_state.clone();
                move |cx| SidebarPlaceholder::new(cx, state.clone())
            });
            let sidebar_dock = cx.new(|cx| {
                DockPanel::new(cx, DockPosition::Left, "Mail")
                    .size(280.0)
                    .content(sidebar.clone().into())
            });

            let message_list = cx.new({
                let state = initial_state.clone();
                move |cx| MessageListPlaceholder::new(cx, state.clone())
            });
            let reading_pane = cx.new({
                let state = initial_state.clone();
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
                let state = initial_state.clone();
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
                let state = initial_state.clone();
                move |cx| {
                    let mut palette = CommandPalette::new(cx).max_results(14);
                    palette.set_entries(palette_entries(&state), cx);
                    palette
                }
            });

            cx.new(|cx| EmailDesktopApp::new(cx, initial_state.clone(), workspace, palette))
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
                state.last_error = Some(err.to_string());
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

    let messages = if let Some(folder_id) = &selected_folder {
        client
            .list_folder_messages(folder_id)
            .await
            .with_context(|| format!("list messages for {folder_id}"))?
    } else {
        Vec::new()
    };

    Ok(NativeAppState {
        accounts,
        folders_by_account,
        labels,
        messages,
        selected_folder,
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
    gap: f32,
}

impl DensityTokens {
    fn new(level: u8) -> Self {
        let level = level.min(8);
        let relaxed = level as f32;
        Self {
            level,
            sidebar_row_height: 28.0 + relaxed * 2.0,
            message_row_height: 52.0 + relaxed * 4.0,
            gap: 4.0 + relaxed,
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
    selected_event: Option<String>,
    messages: Vec<Message>,
    accounts: Vec<Account>,
    labels: Vec<Label>,
    folders_by_account: BTreeMap<String, Vec<Folder>>,
    compose: ComposeState,
    modals: ModalStates,
    palette: PaletteState,
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
            selected_event: None,
            messages: Vec::new(),
            accounts: Vec::new(),
            labels: Vec::new(),
            folders_by_account: BTreeMap::new(),
            compose: ComposeState::default(),
            modals: ModalStates::default(),
            palette: PaletteState::default(),
            last_error: None,
        }
    }
}

struct EmailDesktopApp {
    state: NativeAppState,
    workspace: Entity<Workspace>,
    palette: Entity<CommandPalette>,
}

impl EmailDesktopApp {
    fn new(
        cx: &mut Context<Self>,
        state: NativeAppState,
        workspace: Entity<Workspace>,
        palette: Entity<CommandPalette>,
    ) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self {
            state,
            workspace,
            palette,
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
                    app.state.palette.open = true;
                    app.palette.update(cx, |palette, cx| palette.show(cx));
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &ApplyLightTheme, _, cx| {
                    app.state.theme = ThemeMode::Light;
                    Theme::apply_light(cx);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &ApplyDarkTheme, _, cx| {
                    app.state.theme = ThemeMode::Dark;
                    Theme::apply_dark(cx);
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &SelectMailView, _, cx| {
                    app.state.selected_view = SelectedView::Mail;
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &SelectCalendarView, _, cx| {
                    app.state.selected_view = SelectedView::Calendar;
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &IncreaseDensity, _, cx| {
                    app.state.density =
                        DensityTokens::new(app.state.density.level.saturating_add(1));
                    cx.notify();
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &DecreaseDensity, _, cx| {
                    app.state.density =
                        DensityTokens::new(app.state.density.level.saturating_sub(1));
                    cx.notify();
                }),
            )
            .child(self.workspace.clone())
            .child(self.palette.clone())
    }
}

#[derive(Clone)]
struct SidebarPlaceholder {
    state: NativeAppState,
}

impl SidebarPlaceholder {
    fn new(cx: &mut Context<Self>, state: NativeAppState) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self { state }
    }
}

impl Render for SidebarPlaceholder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let typography = theme.typography;
        let density = self.state.density.clone();

        let mut account_list = div().flex().flex_col().gap(px(density.gap));
        for account in &self.state.accounts {
            let folders = self
                .state
                .folders_by_account
                .get(&account.id)
                .map(|folders| folders.len())
                .unwrap_or_default();
            account_list = account_list.child(
                div()
                    .min_h(px(density.sidebar_row_height))
                    .flex()
                    .flex_col()
                    .justify_center()
                    .px(px(10.0))
                    .rounded(px(4.0))
                    .bg(colors.neutral)
                    .child(
                        div()
                            .text_size(px(typography.body.size))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.on_neutral)
                            .child(account.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(typography.caption.size))
                            .text_color(colors.on_subtle)
                            .child(format!("{} folders - {}", folders, account.email)),
                    ),
            );
        }

        if self.state.accounts.is_empty() {
            account_list = account_list.child(
                div()
                    .p(px(10.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(colors.stroke_neutral_subtle)
                    .text_size(px(typography.body.size))
                    .text_color(colors.on_subtle)
                    .child("No accounts configured"),
            );
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(12.0))
            .bg(colors.panel_bg)
            .child(section_label("Accounts", cx))
            .child(account_list)
            .child(section_label("Labels", cx))
            .child(label_summary(&self.state, cx))
            .child(section_label("Navigation", cx))
            .child(nav_item(
                "Mail",
                self.state.selected_view == SelectedView::Mail,
                cx,
            ))
            .child(nav_item(
                "Calendar",
                self.state.selected_view == SelectedView::Calendar,
                cx,
            ))
    }
}

#[derive(Clone)]
struct MessageListPlaceholder {
    state: NativeAppState,
}

impl MessageListPlaceholder {
    fn new(cx: &mut Context<Self>, state: NativeAppState) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self { state }
    }
}

impl Render for MessageListPlaceholder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let typography = theme.typography;
        let density = self.state.density.clone();

        let mut rows = div().flex().flex_col().gap(px(1.0));
        for message in self.state.messages.iter().take(8) {
            rows = rows.child(
                div()
                    .min_h(px(density.message_row_height))
                    .px(px(14.0))
                    .py(px(8.0))
                    .border_b_1()
                    .border_color(colors.stroke_neutral_subtle)
                    .bg(if message.is_read {
                        colors.surface
                    } else {
                        colors.subtle_hover
                    })
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(typography.body.size))
                                    .font_weight(if message.is_read {
                                        FontWeight::NORMAL
                                    } else {
                                        FontWeight::SEMIBOLD
                                    })
                                    .text_color(colors.on_neutral)
                                    .child(
                                        message
                                            .subject
                                            .clone()
                                            .unwrap_or_else(|| "(no subject)".into()),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(typography.caption.size))
                                    .text_color(colors.on_subtle)
                                    .child(message.date.clone().unwrap_or_default()),
                            ),
                    )
                    .child(
                        div()
                            .pt(px(4.0))
                            .text_size(px(typography.caption.size))
                            .text_color(colors.on_subtle)
                            .child(format!(
                                "{} - {}",
                                message.from_email.clone().unwrap_or_default(),
                                message.preview.clone().unwrap_or_default()
                            )),
                    ),
            );
        }

        if self.state.messages.is_empty() {
            rows = rows.child(empty_panel(
                "No messages loaded for the selected folder",
                cx,
            ));
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors.surface)
            .child(
                div()
                    .px(px(18.0))
                    .py(px(14.0))
                    .border_b_1()
                    .border_color(colors.stroke_neutral_subtle)
                    .child(
                        div()
                            .text_size(px(typography.title.size))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.on_neutral)
                            .child("Native mail skeleton"),
                    )
                    .child(
                        div()
                            .pt(px(4.0))
                            .text_size(px(typography.body.size))
                            .text_color(colors.on_subtle)
                            .child(format!(
                                "{} accounts, {} messages loaded from {}",
                                self.state.accounts.len(),
                                self.state.messages.len(),
                                self.state.api_base_url
                            )),
                    ),
            )
            .child(
                div().flex_1().min_h_0().child(
                    div()
                        .id("message-list-scroll")
                        .overflow_y_scroll()
                        .child(rows),
                ),
            )
    }
}

#[derive(Clone)]
struct ReadingPanePlaceholder {
    state: NativeAppState,
}

impl ReadingPanePlaceholder {
    fn new(cx: &mut Context<Self>, state: NativeAppState) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self { state }
    }
}

impl Render for ReadingPanePlaceholder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        reading_placeholder(&self.state, cx)
    }
}

#[derive(Clone)]
struct StatusBar {
    state: NativeAppState,
}

impl StatusBar {
    fn new(cx: &mut Context<Self>, state: NativeAppState) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        Self { state }
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let typography = theme.typography;
        let status = self
            .state
            .last_error
            .clone()
            .unwrap_or_else(|| "Axum in-process server ready".to_string());

        div()
            .h(px(28.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .border_t_1()
            .border_color(colors.stroke_neutral_subtle)
            .bg(colors.panel_bg)
            .text_size(px(typography.caption.size))
            .text_color(if self.state.last_error.is_some() {
                colors.status_error
            } else {
                colors.on_subtle
            })
            .child(format!(
                "{} | density {} | {:?}",
                status, self.state.density.level, self.state.theme
            ))
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
            .subtitle("Open the calendar placeholder")
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
        .pt(px(4.0))
        .text_size(px(theme.typography.caption.size))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.colors.on_subtle)
        .child(label)
}

fn label_summary(state: &NativeAppState, cx: &mut Context<SidebarPlaceholder>) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let text = if state.labels.is_empty() {
        "No labels loaded".to_string()
    } else {
        state
            .labels
            .iter()
            .map(|label| label.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };

    div()
        .p(px(10.0))
        .rounded(px(4.0))
        .bg(colors.neutral)
        .text_size(px(typography.body.size))
        .text_color(colors.on_neutral)
        .child(text)
}

fn nav_item(
    label: &'static str,
    selected: bool,
    cx: &mut Context<SidebarPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    div()
        .h(px(32.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .bg(if selected {
            colors.accent
        } else {
            colors.neutral
        })
        .text_color(if selected {
            colors.on_accent
        } else {
            colors.on_neutral
        })
        .child(label)
}

fn empty_panel(label: &'static str, cx: &mut Context<MessageListPlaceholder>) -> gpui::Div {
    let theme = cx.theme();
    div()
        .m(px(16.0))
        .p(px(16.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(theme.colors.stroke_neutral_subtle)
        .text_color(theme.colors.on_subtle)
        .child(label)
}

fn reading_placeholder(
    state: &NativeAppState,
    cx: &mut Context<ReadingPanePlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let selected = state
        .selected_message
        .as_deref()
        .unwrap_or("No selected message yet");

    div()
        .size_full()
        .flex()
        .flex_col()
        .border_l_1()
        .border_color(colors.stroke_neutral_subtle)
        .p(px(18.0))
        .gap(px(10.0))
        .child(
            div()
                .text_size(px(typography.subtitle.size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.on_neutral)
                .child("Reading pane placeholder"),
        )
        .child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child(selected.to_string()),
        )
        .child(
            div()
                .mt(px(10.0))
                .p(px(12.0))
                .rounded(px(6.0))
                .bg(colors.neutral)
                .text_size(px(typography.body.size))
                .text_color(colors.on_neutral)
                .child("Compose, modals, command palette, calendar, and full reading pane behavior land in later phases."),
        )
}
