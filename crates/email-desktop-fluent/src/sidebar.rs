use crate::prelude::*;
use crate::*;

#[derive(Clone)]
pub(crate) struct SidebarPlaceholder {
    pub(crate) state: Entity<NativeAppState>,
    pub(crate) searchbox: Entity<Searchbox>,
    pub(crate) context_menu: Option<Entity<ContextMenu>>,
}

impl SidebarPlaceholder {
    pub(crate) fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
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
                            submit_or_select_sidebar_search(&state, query.to_string(), app_cx);
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

        if initial_accounts_loading(&state) {
            account_list = account_list.child(sidebar_loading_accounts(
                density.sidebar_row_height.max(28.0),
                cx,
            ));
        } else if state.accounts.is_empty() {
            account_list = account_list.child(empty_sidebar_text("No accounts configured", cx));
        }

        let unread = total_unread_count(&state);
        let search_suggestions = sidebar_search_suggestions(&self.state, &state, cx);

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
                            .relative()
                            .mb(px(10.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .on_key_down({
                                                let state_entity = self.state.clone();
                                                move |ev: &KeyDownEvent, _, app_cx| {
                                                    if handle_sidebar_search_suggestion_key(
                                                        &state_entity,
                                                        ev.keystroke.key.as_str(),
                                                        app_cx,
                                                    ) {
                                                        app_cx.stop_propagation();
                                                    }
                                                }
                                            })
                                            .child(self.searchbox.clone()),
                                    )
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
                            .when_some(search_suggestions, |area, suggestions| {
                                area.child(suggestions.absolute().top(px(34.0)).left_0().right_0())
                            }),
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
            .when(sync_status_visible(&state.sync), |sidebar| {
                sidebar.child(sync_status_strip(&state.sync, cx))
            })
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

pub(crate) fn section_label(
    label: &'static str,
    cx: &mut Context<SidebarPlaceholder>,
) -> gpui::Div {
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

pub(crate) fn account_tree_header(
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

pub(crate) fn empty_sidebar_text(
    message: &'static str,
    cx: &mut Context<SidebarPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    div()
        .px(px(8.0))
        .py(px(6.0))
        .text_size(px(theme.typography.body.size))
        .text_color(theme.colors.on_subtle)
        .child(message)
}

pub(crate) fn sidebar_loading_accounts(
    row_height: f32,
    cx: &mut Context<SidebarPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let mut rows = div()
        .px(px(8.0))
        .py(px(6.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("Loading accounts..."),
        );

    for width in [0.82, 0.64, 0.74] {
        rows = rows.child(
            div()
                .h(px((row_height * 0.58).max(18.0)))
                .w(relative(width))
                .rounded(px(3.0))
                .bg(colors.neutral),
        );
    }

    rows
}

pub(crate) fn initial_accounts_loading(state: &NativeAppState) -> bool {
    state.accounts_resource.is_loading() && state.accounts.is_empty()
}

pub(crate) fn sync_status_strip(
    status: &NativeSyncStatus,
    cx: &mut Context<SidebarPlaceholder>,
) -> gpui::Div {
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

pub(crate) fn sidebar_footer(
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
                            start_calendar_view(&state_entity, app_cx);
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

pub(crate) fn smart_folder_rows(
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

pub(crate) fn label_rows(
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
            sidebar_label_row(label.name.clone(), selected, label.color.clone(), cx)
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

pub(crate) fn sidebar_label_row(
    label: String,
    selected: bool,
    color: String,
    cx: &mut Context<SidebarPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let dot_color = parse_label_color(&color).unwrap_or(colors.accent);

    div()
        .min_h(px(28.0))
        .pl(px(8.0))
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
                .gap(px(8.0))
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
                .child(
                    div()
                        .flex_none()
                        .size(px(8.0))
                        .rounded(px(999.0))
                        .bg(dot_color),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label),
                ),
        )
}

pub(crate) fn parse_label_color(value: &str) -> Option<Hsla> {
    let trimmed = value.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    let expanded = match hex.len() {
        3 => {
            let mut out = String::with_capacity(6);
            for ch in hex.chars() {
                out.push(ch);
                out.push(ch);
            }
            out
        }
        6 => hex.to_string(),
        _ => return None,
    };
    u32::from_str_radix(&expanded, 16)
        .ok()
        .map(|value| rgb(value).into())
}

pub(crate) fn sidebar_nav_row(
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

pub(crate) fn folder_label(folder: &Folder) -> String {
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

pub(crate) fn folder_icon(folder: &Folder) -> &'static str {
    match folder.special_use.as_deref() {
        Some("trash") => "icons/delete.svg",
        Some("sent") | Some("drafts") => "icons/document.svg",
        Some("archive") => "icons/download.svg",
        Some("spam") => "icons/dismiss.svg",
        _ => "icons/folder.svg",
    }
}

pub(crate) fn smart_folder_icon(kind: &str) -> &'static str {
    match kind {
        "flagged" => "icons/pin.svg",
        "snoozed" => "icons/bell.svg",
        "unread" => "icons/document.svg",
        _ => "icons/list_tree.svg",
    }
}

pub(crate) fn folder_indent(folder: &Folder) -> usize {
    1 + folder.full_path.matches('/').count().min(3)
}

pub(crate) fn nav_item(
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
