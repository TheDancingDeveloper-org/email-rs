mod account_actions;
mod account_setup;
mod advanced_search;
mod app;
mod attachments;
mod auth_modal;
mod bootstrap;
mod calendar;
mod calendar_actions;
mod compose;
mod compose_view;
mod conversation;
mod export;
mod message_actions;
mod message_list;
mod message_move;
mod palette;
mod preferences;
mod prelude;
mod reading_pane;
mod send_undo;
mod settings;
mod settings_actions;
mod sidebar;
mod sidebar_search;
mod state;
mod status_bar;
mod sync_events;
mod system;

#[allow(unused_imports)]
use account_actions::*;
#[allow(unused_imports)]
use account_setup::*;
#[allow(unused_imports)]
use advanced_search::*;
#[allow(unused_imports)]
use app::*;
#[allow(unused_imports)]
use attachments::*;
#[allow(unused_imports)]
use auth_modal::*;
#[allow(unused_imports)]
use bootstrap::*;
#[allow(unused_imports)]
use calendar::*;
#[allow(unused_imports)]
use calendar_actions::*;
#[allow(unused_imports)]
use compose::*;
#[allow(unused_imports)]
use compose_view::*;
#[allow(unused_imports)]
use conversation::*;
#[allow(unused_imports)]
use export::*;
#[allow(unused_imports)]
use message_actions::*;
#[allow(unused_imports)]
use message_list::*;
#[allow(unused_imports)]
use message_move::*;
#[allow(unused_imports)]
use palette::*;
#[allow(unused_imports)]
use preferences::*;
#[allow(unused_imports)]
use prelude::*;
#[allow(unused_imports)]
use reading_pane::*;
#[allow(unused_imports)]
use send_undo::*;
#[allow(unused_imports)]
use settings::*;
#[allow(unused_imports)]
use settings_actions::*;
#[allow(unused_imports)]
use sidebar::*;
#[allow(unused_imports)]
use sidebar_search::*;
#[allow(unused_imports)]
use state::*;
#[allow(unused_imports)]
use status_bar::*;
#[allow(unused_imports)]
use sync_events::*;
#[allow(unused_imports)]
use system::*;

fn main() -> Result<()> {
    // SQLite creates the database, WAL, SHM and migration backups with the
    // process umask. Match the server: owner-only from the first create, so a
    // later path that forgets to chmod them is still not world-readable.
    // Unix only; Windows has no umask. libc's mode_t is u16 on macOS, so the
    // mask goes through libc rather than a u32 declaration.
    restrict_process_umask();
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
    let stored_token = load_native_auth_token();
    set_default_bearer_token(stored_token.clone());
    let mut initial_state = NativeAppState {
        api_base_url: base_url.clone(),
        auth_required: stored_token.is_none(),
        ..NativeAppState::default()
    };
    let preferences = load_native_preferences();
    preferences.apply_to_state(&mut initial_state);
    let initial_layout = initial_state.layout;

    let fluent_app = FluentApp::new(native_window_title(&initial_state))
        .app_id(APP_ID)
        .assets(NativeFluentAssets)
        .window_size(1280.0, 800.0)
        .window_min_size(800.0, 600.0)
        .window_status(native_window_status(&initial_state));
    let fluent_app = match initial_state.theme {
        ThemeMode::Dark => fluent_app.dark_theme(),
        ThemeMode::Light => fluent_app.light_theme(),
    };

    fluent_app.run(move |cx| {
        cx.bind_keys(native_key_bindings());

        let state_entity = cx.new(|_| initial_state.clone());
        let shutdown_state = state_entity.clone();
        let _ = cx.on_app_quit(move |cx| {
            let pending = shutdown_state.update(cx, |state, _| {
                state.pending_undo.take().map(|pending| {
                    let request = begin_sync_complete_refresh(state);
                    (state.api_base_url.clone(), request, pending.mutation)
                })
            });
            let task = pending.map(|(base_url, request, mutation)| {
                cx.background_executor()
                    .spawn(async move { mutate_message_and_refresh(base_url, mutation, request) })
            });
            async move {
                // App shutdown waits for this future, ensuring the mutation
                // reaches the server even when the five-second undo timer has
                // not fired yet.
                if let Some(task) = task {
                    let _ = task.await;
                }
            }
        });
        start_initial_account_load(&state_entity, cx);

        let modal_host = cx.new(ModalHost::new);
        let sidebar = cx.new({
            let state = state_entity.clone();
            move |cx| SidebarPlaceholder::new(cx, state.clone())
        });
        let sidebar_dock = cx.new(|cx| {
            DockPanel::new(cx, DockPosition::Left, "Mail")
                .size(initial_layout.sidebar_width as f32)
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
        let calendar = cx.new({
            let state = state_entity.clone();
            move |cx| CalendarPlaceholder::new(cx, state.clone())
        });
        let mail_split = cx.new(|_| {
            PaneGroup::horizontal()
                .first(message_list.clone().into())
                .second(reading_pane.clone().into())
                .ratio(message_list_ratio_for_width(
                    initial_layout.message_list_width,
                    DEFAULT_WINDOW_WIDTH,
                    initial_layout.sidebar_width as f32,
                ))
                .constraints(
                    PaneConstraints::new()
                        .first_min(MIN_MESSAGE_LIST_WIDTH as f32)
                        .second_min(420.0)
                        .first_max(Some(MAX_MESSAGE_LIST_WIDTH as f32)),
                )
        });
        let main_content = cx.new({
            let state = state_entity.clone();
            let mail_split = mail_split.clone();
            move |cx| {
                MainContentPlaceholder::new(cx, state.clone(), mail_split.clone(), calendar.clone())
            }
        });
        let pane = cx.new(Pane::new);
        pane.update(cx, |pane, cx| {
            pane.set_content(main_content.clone().into(), cx);
        });

        let status_bar = cx.new({
            let state = state_entity.clone();
            move |cx| StatusBar::new(cx, state.clone())
        });
        let title_bar = cx.new(|cx| TitleBar::from_window_title(cx).icon(APP_ICON_PATH));

        let workspace = cx.new(|cx| {
            Workspace::new(cx)
                .title_bar(title_bar.clone().into())
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

        cx.new(|cx| {
            EmailDesktopApp::new(
                cx,
                state_entity.clone(),
                workspace,
                palette,
                sidebar_dock.clone(),
                mail_split.clone(),
            )
        })
    });

    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .try_init();
}

#[cfg(test)]
mod gui_tests;
#[cfg(test)]
mod tests;
