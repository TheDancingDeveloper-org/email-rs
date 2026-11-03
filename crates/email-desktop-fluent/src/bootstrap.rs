use crate::prelude::*;
use crate::*;

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
        OpenSettings,
        ArchiveSelected,
        ToggleSelectedStar,
        ToggleSelectedRead,
        ToggleShortcutHelp,
        CyclePaneFocus
    ]
);

pub(crate) const APP_TITLE: &str = "email-rs FluentGUI";
pub(crate) const APP_ID: &str = "org.email-rs.desktop.fluent";
pub(crate) const APP_ICON_PATH: &str = "icons/email-rs.svg";
pub(crate) const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const NATIVE_SETTINGS_ENV: &str = "EMAIL_RS_FLUENT_SETTINGS_PATH";
pub(crate) const NATIVE_SETTINGS_FILE: &str = "fluentgui-settings.conf";
pub(crate) const NATIVE_AUTH_FILE: &str = "fluentgui-session.token";
pub(crate) const DEFAULT_WINDOW_WIDTH: f32 = 1280.0;
pub(crate) const DEFAULT_SIDEBAR_WIDTH: u16 = 280;
pub(crate) const MIN_SIDEBAR_WIDTH: u16 = 80;
pub(crate) const MAX_SIDEBAR_WIDTH: u16 = 700;
pub(crate) const DEFAULT_MESSAGE_LIST_WIDTH: u16 = 420;
pub(crate) const MIN_MESSAGE_LIST_WIDTH: u16 = 320;
pub(crate) const MAX_MESSAGE_LIST_WIDTH: u16 = 640;
pub(crate) const SYNC_COMPLETE_VISIBLE_FOR: Duration = Duration::from_secs(3);
pub(crate) const SEARCH_RESULTS_DEBOUNCE: Duration = Duration::from_millis(300);
pub(crate) const CALENDAR_HOUR_HEIGHT: f32 = 48.0;
pub(crate) const CALENDAR_DAY_HEIGHT: f32 = CALENDAR_HOUR_HEIGHT * 24.0;
pub(crate) const CALENDAR_INITIAL_SCROLL_HOUR: usize = 8;
pub(crate) const MIN_TIMED_EVENT_HEIGHT: f32 = 18.0;

pub(crate) const APP_ICON_SVG: &[u8] = br##"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg"><path fill="#5b5fc7" d="M2 4.5A1.5 1.5 0 0 1 3.5 3h9A1.5 1.5 0 0 1 14 4.5v7a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z"/><path fill="#fff" d="M3.4 5.1 8 8.1l4.6-3 .6.9L8 9.4 2.8 6z"/></svg>"##;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NativeFluentAssets;

impl gpui::AssetSource for NativeFluentAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok((path == APP_ICON_PATH).then_some(Cow::Borrowed(APP_ICON_SVG)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok((path == "icons")
            .then_some(SharedString::from("email-rs.svg"))
            .into_iter()
            .collect())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeKeyAction {
    TogglePalette,
    ApplyLightTheme,
    ApplyDarkTheme,
    IncreaseDensity,
    DecreaseDensity,
    Reply,
    Archive,
    Forward,
    Compose,
    Search,
    Star,
    MarkRead,
    Help,
    CyclePane,
}

pub(crate) const SHORTCUT_COVERAGE: &[(&str, &str, &str)] = &[
    ("r", "Reply", "Gmail / Superhuman"),
    ("a", "Archive", "Gmail / Superhuman"),
    ("f", "Forward", "Gmail / Superhuman"),
    ("c", "Compose", "Gmail / Superhuman"),
    ("/", "Search", "Gmail / Superhuman"),
    ("s", "Star", "Gmail"),
    ("l", "Mark read", "Gmail / Superhuman"),
    ("?", "Shortcut help", "Gmail / Superhuman"),
    ("f6", "Cycle pane", "Superhuman"),
];

pub(crate) fn native_key_binding_specs() -> &'static [(&'static str, NativeKeyAction)] {
    &[
        ("ctrl-k", NativeKeyAction::TogglePalette),
        ("cmd-k", NativeKeyAction::TogglePalette),
        ("ctrl-shift-l", NativeKeyAction::ApplyLightTheme),
        ("ctrl-shift-d", NativeKeyAction::ApplyDarkTheme),
        ("ctrl-plus", NativeKeyAction::IncreaseDensity),
        ("ctrl-minus", NativeKeyAction::DecreaseDensity),
        ("r", NativeKeyAction::Reply),
        ("a", NativeKeyAction::Archive),
        ("f", NativeKeyAction::Forward),
        ("c", NativeKeyAction::Compose),
        ("/", NativeKeyAction::Search),
        ("s", NativeKeyAction::Star),
        ("l", NativeKeyAction::MarkRead),
        ("shift-?", NativeKeyAction::Help),
        ("f6", NativeKeyAction::CyclePane),
    ]
}

pub(crate) fn native_key_bindings() -> Vec<KeyBinding> {
    native_key_binding_specs()
        .iter()
        .map(|(keystroke, action)| match action {
            NativeKeyAction::TogglePalette => KeyBinding::new(keystroke, TogglePalette, None),
            NativeKeyAction::ApplyLightTheme => KeyBinding::new(keystroke, ApplyLightTheme, None),
            NativeKeyAction::ApplyDarkTheme => KeyBinding::new(keystroke, ApplyDarkTheme, None),
            NativeKeyAction::IncreaseDensity => KeyBinding::new(keystroke, IncreaseDensity, None),
            NativeKeyAction::DecreaseDensity => KeyBinding::new(keystroke, DecreaseDensity, None),
            NativeKeyAction::Reply => KeyBinding::new(keystroke, StartReplyCompose, None),
            NativeKeyAction::Archive => KeyBinding::new(keystroke, ArchiveSelected, None),
            NativeKeyAction::Forward => KeyBinding::new(keystroke, StartForwardCompose, None),
            NativeKeyAction::Compose => KeyBinding::new(keystroke, StartCompose, None),
            NativeKeyAction::Search => KeyBinding::new(keystroke, OpenAdvancedSearch, None),
            NativeKeyAction::Star => KeyBinding::new(keystroke, ToggleSelectedStar, None),
            NativeKeyAction::MarkRead => KeyBinding::new(keystroke, ToggleSelectedRead, None),
            NativeKeyAction::Help => KeyBinding::new(keystroke, ToggleShortcutHelp, None),
            NativeKeyAction::CyclePane => KeyBinding::new(keystroke, CyclePaneFocus, None),
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn shortcut_conflicts() -> Vec<&'static str> {
    let mut seen = BTreeSet::new();
    native_key_binding_specs()
        .iter()
        .filter_map(|(key, _)| (!seen.insert(*key)).then_some(*key))
        .collect()
}

pub(crate) fn shortcut_help() -> String {
    SHORTCUT_COVERAGE
        .iter()
        .map(|(key, action, products)| format!("{key:>5}  {action:<16} {products}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(unix)]
pub(crate) fn restrict_process_umask() {
    let _ = unsafe { libc::umask(0o077) };
}

#[cfg(not(unix))]
pub(crate) fn restrict_process_umask() {}

pub(crate) fn should_run_headless() -> bool {
    #[cfg(target_os = "linux")]
    {
        headless_mode_for_environment(
            std::env::var("EMAIL_RS_HEADLESS").is_ok(),
            std::env::var("DISPLAY").is_ok(),
            std::env::var("WAYLAND_DISPLAY").is_ok(),
        )
    }

    #[cfg(not(target_os = "linux"))]
    {
        headless_mode_for_environment(std::env::var("EMAIL_RS_HEADLESS").is_ok())
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn headless_mode_for_environment(
    forced: bool,
    has_display: bool,
    has_wayland_display: bool,
) -> bool {
    forced || (!has_display && !has_wayland_display)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn headless_mode_for_environment(forced: bool) -> bool {
    forced
}

pub(crate) fn start_server_thread(cfg: Config) -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
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

pub(crate) fn wait_for_server(bind_addr: &str, server_errors: &Receiver<String>) -> Result<()> {
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

pub(crate) fn start_initial_account_load(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    let (base_url, accounts_request, messages_request) =
        state_entity.update(app_cx, |state, cx| {
            state.accounts.clear();
            state.folders_by_account.clear();
            state.labels.clear();
            state.messages.clear();
            state.selected_folder = None;
            state.selected_message = None;
            state.selected_full_message = None;
            state.selected_message_resource.reset();
            state.attachment_busy = false;
            state.attachment_notice = None;
            state.attachment_error = None;
            state.search_event_results.clear();
            state.last_error = None;
            let accounts_request = state.accounts_resource.begin_load();
            let messages_request = state.messages_resource.begin_load();
            cx.notify();
            (
                state.api_base_url.clone(),
                accounts_request,
                messages_request,
            )
        });

    let task = app_cx
        .background_executor()
        .spawn(async move { reload_accounts(base_url) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(loaded) => {
                            apply_initial_account_load(
                                state,
                                accounts_request,
                                messages_request,
                                loaded,
                            );
                        }
                        Err(err) => {
                            reject_initial_account_load(
                                state,
                                accounts_request,
                                messages_request,
                                err,
                            );
                        }
                    }
                    sync_window_title(state, cx);
                    cx.notify();
                });
            }
        })
        .detach();
}

pub(crate) async fn load_accounts(client: &EmailClient) -> Result<NativeAppState> {
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
pub(crate) struct LoadedAccountTree {
    pub(crate) accounts: Vec<Account>,
    pub(crate) folders_by_account: BTreeMap<String, Vec<Folder>>,
    pub(crate) labels: Vec<Label>,
}

pub(crate) async fn load_account_tree(client: &EmailClient) -> Result<LoadedAccountTree> {
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
