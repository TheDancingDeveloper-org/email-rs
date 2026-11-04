use crate::prelude::*;
use crate::*;

pub(crate) struct EmailDesktopApp {
    pub(crate) state: Entity<NativeAppState>,
    pub(crate) workspace: Entity<Workspace>,
    pub(crate) palette: Entity<CommandPalette>,
    pub(crate) sidebar_dock: Entity<DockPanel>,
    pub(crate) mail_split: Entity<PaneGroup>,
    pub(crate) account_setup_modal: Entity<AccountSetupModal>,
    pub(crate) advanced_search_modal: Entity<AdvancedSearchModal>,
    pub(crate) settings_modal: Entity<SettingsModal>,
    pub(crate) auth_modal: Entity<NativeAuthModal>,
    pub(crate) sync_updates: Receiver<NativeSyncUpdate>,
    pub(crate) last_window_width: f32,
}

impl EmailDesktopApp {
    pub(crate) fn new(
        cx: &mut Context<Self>,
        state: Entity<NativeAppState>,
        workspace: Entity<Workspace>,
        palette: Entity<CommandPalette>,
        sidebar_dock: Entity<DockPanel>,
        mail_split: Entity<PaneGroup>,
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
        let auth_modal = cx.new({
            let state = state.clone();
            move |cx| NativeAuthModal::new(cx, state.clone())
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
                        let _ = app.update(&mut async_cx, |app, cx| {
                            app.state.update(cx, |state, cx| {
                                if expire_move_notice(&mut state.move_notice, Instant::now()) {
                                    cx.notify();
                                }
                            });
                        });
                        if let Ok(Some(mutation)) =
                            app.update(&mut async_cx, |app, cx| take_expired_undo(&app.state, cx))
                        {
                            let _ = app.update(&mut async_cx, |app, cx| {
                                commit_message_mutation_and_refresh(&app.state, mutation, cx);
                            });
                        }
                    }
                }
            },
        )
        .detach();
        cx.observe(&sidebar_dock, |app, _, cx| {
            app.persist_sidebar_width_preference(cx)
        })
        .detach();
        cx.observe(&mail_split, |app, _, cx| {
            app.persist_message_list_width_preference(cx)
        })
        .detach();
        Self {
            state,
            workspace,
            palette,
            sidebar_dock,
            mail_split,
            account_setup_modal,
            advanced_search_modal,
            settings_modal,
            auth_modal,
            sync_updates,
            last_window_width: DEFAULT_WINDOW_WIDTH,
        }
    }

    pub(crate) fn drain_sync_updates(&mut self, cx: &mut Context<Self>) {
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
        let expired = self.state.update(cx, |state, cx| {
            if expire_sync_completion(&mut state.sync, Instant::now()) {
                cx.notify();
                true
            } else {
                false
            }
        });
        let debounced_search = self.state.update(cx, |state, cx| {
            let source = take_due_debounced_search(&mut state.search, Instant::now());
            if source.is_some() {
                cx.notify();
            }
            source
        });
        if refresh_requested {
            start_sync_complete_refresh(&self.state, cx);
        }
        if let Some(source) = debounced_search {
            start_message_collection_load(&self.state, source, cx);
        }
        if changed || expired {
            cx.notify();
        }
    }

    pub(crate) fn apply_theme_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
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

    pub(crate) fn adjust_density(&mut self, delta: i8, cx: &mut Context<Self>) {
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

    pub(crate) fn persist_preferences(
        &mut self,
        preferences: NativePreferences,
        cx: &mut Context<Self>,
    ) {
        if let Err(err) = save_native_preferences(&preferences) {
            tracing::warn!("failed to save native FluentGUI settings: {err:#}");
            self.state.update(cx, |state, cx| {
                state.last_error = Some(format!("Failed to save native settings: {err}"));
                cx.notify();
            });
        }
    }

    pub(crate) fn persist_sidebar_width_preference(&mut self, cx: &mut Context<Self>) {
        let sidebar_width = clamp_layout_width(
            self.sidebar_dock.read(cx).size,
            MIN_SIDEBAR_WIDTH,
            MAX_SIDEBAR_WIDTH,
        );
        let current_message_width = self.state.read(cx).layout.message_list_width;
        let next_ratio = message_list_ratio_for_width(
            current_message_width,
            self.last_window_width,
            sidebar_width as f32,
        );
        let preferences = self.state.update(cx, |state, cx| {
            if state.layout.sidebar_width == sidebar_width {
                return None;
            }
            state.layout.sidebar_width = sidebar_width;
            cx.notify();
            Some(NativePreferences::from_state(state))
        });

        if let Some(preferences) = preferences {
            self.mail_split
                .update(cx, |split, cx| split.set_ratio(next_ratio, cx));
            self.persist_preferences(preferences, cx);
        }
    }

    pub(crate) fn persist_message_list_width_preference(&mut self, cx: &mut Context<Self>) {
        let ratio = self.mail_split.read(cx).ratio_value();
        let sidebar_width = self.state.read(cx).layout.sidebar_width as f32;
        let message_list_width =
            message_list_width_for_ratio(ratio, self.last_window_width, sidebar_width);
        let preferences = self.state.update(cx, |state, cx| {
            if state.layout.message_list_width == message_list_width {
                return None;
            }
            state.layout.message_list_width = message_list_width;
            cx.notify();
            Some(NativePreferences::from_state(state))
        });

        if let Some(preferences) = preferences {
            self.persist_preferences(preferences, cx);
        }
    }

    pub(crate) fn refresh_palette_entries(&mut self, cx: &mut Context<Self>) {
        let state = self.state.read(cx).clone();
        let state_entity = self.state.clone();
        self.palette.update(cx, |palette, cx| {
            palette.set_entries(palette_entries(&state, &state_entity), cx)
        });
    }
}

impl Render for EmailDesktopApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window_width = f32::from(window.bounds().size.width);
        if window_width.is_finite() {
            self.last_window_width = window_width.max(1.0);
        }

        let auth_required = self.state.read(cx).auth_required;
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
                    start_calendar_view(&app.state, cx);
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
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &ArchiveSelected, _, cx| {
                    let id = app.state.read(cx).selected_message.clone();
                    if let Some(message_id) = id {
                        start_message_mutation_and_refresh(
                            &app.state,
                            NativeMessageMutation::Archive { message_id },
                            cx,
                        );
                    }
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &ToggleSelectedStar, _, cx| {
                    let state = app.state.read(cx);
                    let args = state
                        .selected_message
                        .clone()
                        .zip(state.selected_full_message.as_ref().map(|m| m.is_flagged));
                    if let Some((message_id, flagged)) = args {
                        start_message_mutation_and_refresh(
                            &app.state,
                            NativeMessageMutation::SetFlagged {
                                message_id,
                                is_flagged: !flagged,
                            },
                            cx,
                        );
                    }
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &ToggleSelectedRead, _, cx| {
                    let state = app.state.read(cx);
                    let args = state
                        .selected_message
                        .clone()
                        .zip(state.selected_full_message.as_ref().map(|m| m.is_read));
                    if let Some((message_id, read)) = args {
                        start_message_mutation_and_refresh(
                            &app.state,
                            NativeMessageMutation::SetRead {
                                message_id,
                                is_read: !read,
                            },
                            cx,
                        );
                    }
                }),
            )
            .on_action(
                cx.listener(|app: &mut EmailDesktopApp, _: &ToggleShortcutHelp, _, cx| {
                    app.state.update(cx, |state, cx| {
                        state.show_shortcut_help = !state.show_shortcut_help;
                        cx.notify();
                    });
                }),
            )
            .on_action(cx.listener(
                |_app: &mut EmailDesktopApp, _: &CyclePaneFocus, window, cx| {
                    window.focus_next(cx);
                },
            ))
            .when(!auth_required, |root| root.child(self.workspace.clone()))
            .when(self.state.read(cx).show_shortcut_help, |root| {
                root.child(
                    div()
                        .absolute()
                        .top(px(80.))
                        .right(px(80.))
                        .w(px(430.))
                        .p(px(20.))
                        .bg(cx.theme().colors.surface)
                        .border_1()
                        .border_color(cx.theme().colors.stroke_neutral_subtle)
                        .rounded(px(8.))
                        .child(div().font_family("monospace").child(shortcut_help())),
                )
            })
            .child(self.palette.clone())
            .child(self.account_setup_modal.clone())
            .child(self.advanced_search_modal.clone())
            .child(self.settings_modal.clone())
            .child(self.auth_modal.clone())
    }
}

#[derive(Clone)]
pub(crate) struct MainContentPlaceholder {
    pub(crate) state: Entity<NativeAppState>,
    pub(crate) mail_split: Entity<PaneGroup>,
    pub(crate) calendar: Entity<CalendarPlaceholder>,
}

impl MainContentPlaceholder {
    pub(crate) fn new(
        cx: &mut Context<Self>,
        state: Entity<NativeAppState>,
        mail_split: Entity<PaneGroup>,
        calendar: Entity<CalendarPlaceholder>,
    ) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            mail_split,
            calendar,
        }
    }
}

impl Render for MainContentPlaceholder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.state.read(cx).selected_view == SelectedView::Calendar {
            self.calendar.clone().into_any_element()
        } else {
            self.mail_split.clone().into_any_element()
        }
    }
}
