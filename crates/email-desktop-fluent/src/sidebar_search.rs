use crate::prelude::*;
use crate::*;

pub(crate) fn load_sidebar_search_suggestions(
    base_url: String,
    query: String,
) -> Result<Vec<NativeSearchSuggestion>> {
    thread::sleep(Duration::from_millis(150));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        let response = client
            .suggest(&query, None)
            .await
            .with_context(|| format!("load search suggestions for {query}"))?;
        let mut suggestions = response
            .messages
            .into_iter()
            .map(NativeSearchSuggestion::Message)
            .collect::<Vec<_>>();
        suggestions.extend(
            response
                .events
                .into_iter()
                .map(NativeSearchSuggestion::Event),
        );
        Ok(suggestions)
    })
}

pub(crate) fn apply_loaded_sidebar_search_suggestions(
    state: &mut NativeAppState,
    request: ResourceRequest,
    query: &str,
    suggestions: Vec<NativeSearchSuggestion>,
) -> bool {
    if state.search.query.trim() != query
        || state.search.suggestions_query.as_deref() != Some(query)
    {
        return false;
    }
    if !state
        .search
        .suggestions_resource
        .resolve(request, suggestions.clone())
    {
        return false;
    }

    state.search.suggestions = suggestions;
    state.search.suggestions_open = !state.search.suggestions.is_empty();
    state.search.suggestion_list.set_highlighted(None);
    true
}

pub(crate) fn reject_sidebar_search_suggestions_load(
    state: &mut NativeAppState,
    request: ResourceRequest,
    query: &str,
    err: anyhow::Error,
) -> bool {
    if state.search.query.trim() != query
        || state.search.suggestions_query.as_deref() != Some(query)
    {
        return false;
    }
    if !state
        .search
        .suggestions_resource
        .reject(request, err.to_string())
    {
        return false;
    }

    state.search.suggestions.clear();
    state.search.suggestions_open = false;
    state.search.suggestion_list.set_highlighted(None);
    true
}

pub(crate) fn sync_searchbox_value(
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

pub(crate) fn sidebar_search_suggestions(
    state_entity: &Entity<NativeAppState>,
    state: &NativeAppState,
    cx: &mut Context<SidebarPlaceholder>,
) -> Option<gpui::Div> {
    if !state.search.suggestions_open || state.search.query.trim().len() < 2 {
        return None;
    }

    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let loading = state.search.suggestions_resource.is_loading();
    let error = state.search.suggestions_resource.error().cloned();
    if state.search.suggestions.is_empty() && !loading && error.is_none() {
        return None;
    }

    let mut panel = div()
        .border_1()
        .border_color(colors.stroke_neutral)
        .rounded(px(4.0))
        .shadow_md()
        .bg(colors.surface)
        .py(px(4.0));

    if state.search.suggestions.is_empty() {
        let (text, color) = if loading {
            ("Loading...".to_string(), colors.on_subtle)
        } else {
            (
                error.unwrap_or_else(|| "Search suggestions failed.".to_string()),
                colors.status_error,
            )
        };
        return Some(
            panel.child(
                div()
                    .min_h(px(32.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .text_size(px(typography.body.size))
                    .text_color(color)
                    .child(text),
            ),
        );
    }

    for (idx, suggestion) in state.search.suggestions.iter().enumerate() {
        panel = panel.child(sidebar_search_suggestion_row(
            state_entity,
            suggestion,
            state.search.suggestion_list.highlighted() == Some(idx),
            cx,
        ));
    }

    Some(panel)
}

pub(crate) fn sidebar_search_suggestion_row(
    state_entity: &Entity<NativeAppState>,
    suggestion: &NativeSearchSuggestion,
    highlighted: bool,
    cx: &mut Context<SidebarPlaceholder>,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let id = native_search_suggestion_id(suggestion);
    let row_id = id.replace(':', "-");
    let label = native_search_suggestion_label(suggestion);
    let detail = native_search_suggestion_detail(suggestion);
    let meta = native_search_suggestion_meta(suggestion);

    div()
        .id(SharedString::from(format!(
            "sidebar-search-suggestion-{row_id}"
        )))
        .min_h(px(38.0))
        .px(px(10.0))
        .py(px(5.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .bg(if highlighted {
            colors.neutral_selected
        } else {
            colors.surface
        })
        .hover({
            let hover = colors.neutral_hover;
            move |style| style.bg(hover)
        })
        .cursor_pointer()
        .on_mouse_move({
            let state_entity = state_entity.clone();
            let id = id.clone();
            move |_, _, app_cx| {
                highlight_sidebar_search_suggestion(&state_entity, id.clone(), app_cx);
            }
        })
        .on_click({
            let state_entity = state_entity.clone();
            let id = id.clone();
            move |_, _, app_cx| {
                select_sidebar_search_suggestion_by_id(&state_entity, id.clone(), app_cx);
            }
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
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
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors.on_neutral)
                                .child(label),
                        )
                        .when_some(meta, |row, meta| {
                            row.child(
                                div()
                                    .flex_none()
                                    .text_size(px(typography.caption.size))
                                    .text_color(colors.on_subtle)
                                    .child(meta),
                            )
                        }),
                )
                .child(
                    div()
                        .pt(px(1.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(typography.caption.size))
                        .text_color(colors.on_subtle)
                        .child(detail),
                ),
        )
        .into_any_element()
}

pub(crate) fn open_advanced_search_modal(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AdvancedSearchModal>,
    app_cx: &mut App,
) {
    let group = state_entity.update(app_cx, |state, cx| {
        state.modals.advanced_search_open = true;
        cx.notify();
        state.search.condition_group.clone()
    });
    modal_entity.update(app_cx, |modal, cx| {
        modal.reset_to_group(group, cx);
    });
}

pub(crate) fn close_advanced_search_modal(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        state.modals.advanced_search_open = false;
        cx.notify();
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdvancedSearchKeyAction {
    Close,
    Submit,
}

pub(crate) fn advanced_search_key_action(
    key: &str,
    has_active: bool,
) -> Option<AdvancedSearchKeyAction> {
    match key {
        "escape" => Some(AdvancedSearchKeyAction::Close),
        "enter" | "return" if has_active => Some(AdvancedSearchKeyAction::Submit),
        _ => None,
    }
}

pub(crate) fn handle_advanced_search_modal_key(
    state_entity: &Entity<NativeAppState>,
    modal_entity: &Entity<AdvancedSearchModal>,
    key: &str,
    app_cx: &mut App,
) -> bool {
    let (action, group) = {
        let modal = modal_entity.read(app_cx);
        (
            advanced_search_key_action(key, modal.has_active_condition()),
            modal.condition_group(),
        )
    };

    match action {
        Some(AdvancedSearchKeyAction::Close) => {
            close_advanced_search_modal(state_entity, app_cx);
            true
        }
        Some(AdvancedSearchKeyAction::Submit) => {
            start_advanced_search(state_entity, group, app_cx);
            true
        }
        None => false,
    }
}

pub(crate) fn handle_sidebar_search_change(
    state_entity: &Entity<NativeAppState>,
    query: String,
    app_cx: &mut App,
) {
    let should_load_suggestions = query.trim().len() >= 2;
    let restore_source = state_entity.update(app_cx, |state, cx| {
        state.search.query = query.clone();
        if !query.trim().is_empty() {
            state.search.condition_group = None;
        }
        schedule_debounced_search(&mut state.search, &query, Instant::now());
        if !should_load_suggestions {
            clear_sidebar_search_suggestions(&mut state.search);
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
    if should_load_suggestions {
        start_sidebar_search_suggestions_load(state_entity, query.trim().to_string(), app_cx);
    }
}

pub(crate) fn schedule_debounced_search(search: &mut SearchState, query: &str, now: Instant) {
    let query = query.trim();
    search.pending_query = if query.is_empty() {
        None
    } else {
        Some(DebouncedSearch {
            query: query.to_string(),
            due_at: now + SEARCH_RESULTS_DEBOUNCE,
        })
    };
}

pub(crate) fn take_due_debounced_search(
    search: &mut SearchState,
    now: Instant,
) -> Option<NativeMessageSource> {
    let pending = search.pending_query.as_ref()?;
    if now < pending.due_at {
        return None;
    }

    let query = pending.query.clone();
    search.pending_query = None;
    if search.query.trim() != query || search.submitted_query.as_deref() == Some(query.as_str()) {
        return None;
    }

    Some(NativeMessageSource::Search(query))
}

pub(crate) fn submit_sidebar_search(
    state_entity: &Entity<NativeAppState>,
    query: String,
    app_cx: &mut App,
) {
    let query = query.trim().to_string();
    if query.is_empty() {
        handle_sidebar_search_change(state_entity, query, app_cx);
        return;
    }

    start_message_collection_load(state_entity, NativeMessageSource::Search(query), app_cx);
}

pub(crate) fn submit_or_select_sidebar_search(
    state_entity: &Entity<NativeAppState>,
    query: String,
    app_cx: &mut App,
) {
    let selected_id = {
        let state = state_entity.read(app_cx);
        highlighted_sidebar_search_suggestion_id(&state.search)
    };

    if let Some(id) = selected_id {
        select_sidebar_search_suggestion_by_id(state_entity, id, app_cx);
    } else {
        state_entity.update(app_cx, |state, cx| {
            clear_sidebar_search_suggestions(&mut state.search);
            cx.notify();
        });
        submit_sidebar_search(state_entity, query, app_cx);
    }
}

pub(crate) fn start_sidebar_search_suggestions_load(
    state_entity: &Entity<NativeAppState>,
    query: String,
    app_cx: &mut App,
) {
    let (base_url, request) = state_entity.update(app_cx, |state, cx| {
        state.search.suggestions_query = Some(query.clone());
        state.search.suggestions.clear();
        state.search.suggestions_open = true;
        state.search.suggestion_list.set_highlighted(None);
        let request = state.search.suggestions_resource.begin_load();
        cx.notify();
        (state.api_base_url.clone(), request)
    });

    let task_query = query.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { load_sidebar_search_suggestions(base_url, task_query) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(suggestions) => {
                            apply_loaded_sidebar_search_suggestions(
                                state,
                                request,
                                &query,
                                suggestions,
                            );
                        }
                        Err(err) => {
                            reject_sidebar_search_suggestions_load(state, request, &query, err);
                        }
                    }
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native sidebar search suggestions: {err}");
                }
            }
        })
        .detach();
}

pub(crate) fn handle_sidebar_search_suggestion_key(
    state_entity: &Entity<NativeAppState>,
    key: &str,
    app_cx: &mut App,
) -> bool {
    if !matches!(key, "down" | "up" | "home" | "end" | "escape") {
        return false;
    }

    state_entity.update(app_cx, |state, cx| {
        if !state.search.suggestions_open {
            return false;
        }
        if key == "escape" {
            state.search.suggestions_open = false;
            state.search.suggestion_list.set_highlighted(None);
            cx.notify();
            return true;
        }
        let items = sidebar_search_suggestion_items(&state.search.suggestions);
        if items.is_empty() {
            return false;
        }
        match state.search.suggestion_list.handle_key(key, &items) {
            SuggestionKeyAction::Move(_) => {
                cx.notify();
                true
            }
            _ => false,
        }
    })
}

pub(crate) fn highlight_sidebar_search_suggestion(
    state_entity: &Entity<NativeAppState>,
    id: String,
    app_cx: &mut App,
) {
    state_entity.update(app_cx, |state, cx| {
        let items = sidebar_search_suggestion_items(&state.search.suggestions);
        if let Some(index) = items
            .iter()
            .position(|item| item.id.as_ref() == id.as_str())
        {
            state.search.suggestion_list.set_highlighted(Some(index));
            cx.notify();
        }
    });
}

pub(crate) fn select_sidebar_search_suggestion_by_id(
    state_entity: &Entity<NativeAppState>,
    id: String,
    app_cx: &mut App,
) -> bool {
    let suggestion = state_entity.update(app_cx, |state, cx| {
        let suggestion = state
            .search
            .suggestions
            .iter()
            .find(|suggestion| native_search_suggestion_id(suggestion) == id)
            .cloned();
        if suggestion.is_some() {
            clear_sidebar_search_suggestions(&mut state.search);
            cx.notify();
        }
        suggestion
    });

    match suggestion {
        Some(NativeSearchSuggestion::Message(message)) => {
            start_message_collection_load_with_preferred(
                state_entity,
                NativeMessageSource::Folder(message.folder_id),
                Some(message.id),
                app_cx,
            );
            true
        }
        Some(NativeSearchSuggestion::Event(event)) => {
            open_calendar_search_result(state_entity, event.id, event.start_at, app_cx);
            true
        }
        None => false,
    }
}

pub(crate) fn clear_sidebar_search_suggestions(search: &mut SearchState) {
    search.suggestions_query = None;
    search.suggestions.clear();
    search.suggestions_open = false;
    search.suggestions_resource.reset();
    search.suggestion_list.set_highlighted(None);
}

pub(crate) fn sidebar_search_suggestion_items(
    suggestions: &[NativeSearchSuggestion],
) -> Vec<SuggestionItem> {
    suggestions
        .iter()
        .map(|suggestion| {
            let mut item = SuggestionItem::new(
                native_search_suggestion_id(suggestion),
                native_search_suggestion_label(suggestion),
            )
            .detail(native_search_suggestion_detail(suggestion));
            if let Some(meta) = native_search_suggestion_meta(suggestion) {
                item = item.meta(meta);
            }
            item
        })
        .collect()
}

pub(crate) fn highlighted_sidebar_search_suggestion_id(search: &SearchState) -> Option<String> {
    let items = sidebar_search_suggestion_items(&search.suggestions);
    search
        .suggestion_list
        .highlighted_item(&items)
        .map(|item| item.id.to_string())
}

pub(crate) fn native_search_suggestion_id(suggestion: &NativeSearchSuggestion) -> String {
    match suggestion {
        NativeSearchSuggestion::Message(message) => format!("message:{}", message.id),
        NativeSearchSuggestion::Event(event) => format!("event:{}", event.id),
    }
}

pub(crate) fn native_search_suggestion_label(suggestion: &NativeSearchSuggestion) -> String {
    match suggestion {
        NativeSearchSuggestion::Message(message) => message
            .subject
            .as_deref()
            .map(str::trim)
            .filter(|subject| !subject.is_empty())
            .unwrap_or("(no subject)")
            .to_string(),
        NativeSearchSuggestion::Event(event) => {
            if event.title.trim().is_empty() {
                "(untitled event)".to_string()
            } else {
                event.title.clone()
            }
        }
    }
}

pub(crate) fn native_search_suggestion_detail(suggestion: &NativeSearchSuggestion) -> String {
    match suggestion {
        NativeSearchSuggestion::Message(message) => message
            .from_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                message
                    .from_email
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or("Unknown")
            .to_string(),
        NativeSearchSuggestion::Event(_) => "Calendar event".to_string(),
    }
}

pub(crate) fn native_search_suggestion_meta(suggestion: &NativeSearchSuggestion) -> Option<String> {
    match suggestion {
        NativeSearchSuggestion::Message(message) => compact_date_label(message.date.as_deref()),
        NativeSearchSuggestion::Event(event) => compact_date_label(Some(event.start_at.as_str())),
    }
}

pub(crate) fn start_advanced_search(
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
