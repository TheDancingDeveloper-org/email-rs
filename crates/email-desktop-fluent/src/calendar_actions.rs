use crate::prelude::*;
use crate::*;

pub(crate) fn load_calendar_events(
    base_url: String,
    from: chrono::DateTime<Utc>,
    to: chrono::DateTime<Utc>,
) -> Result<Vec<CalendarEvent>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .list_events(None, Some(from), Some(to))
            .await
            .context("list calendar events")
    })
}

pub(crate) fn load_calendar_event_links(
    base_url: String,
    event_id: String,
) -> Result<Vec<EventLink>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .list_event_links(&event_id)
            .await
            .with_context(|| format!("list linked emails for calendar event {event_id}"))
    })
}

pub(crate) fn remove_calendar_event_link(
    base_url: String,
    event_id: String,
    message_id: String,
) -> Result<Vec<EventLink>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let client = EmailClient::new(&base_url)?;
        client
            .remove_event_link(&event_id, &message_id)
            .await
            .with_context(|| {
                format!("unlink message {message_id} from calendar event {event_id}")
            })?;
        client
            .list_event_links(&event_id)
            .await
            .with_context(|| format!("reload linked emails for calendar event {event_id}"))
    })
}

pub(crate) fn apply_loaded_calendar_events(
    state: &mut NativeAppState,
    request: ResourceRequest,
    events: Vec<CalendarEvent>,
) -> bool {
    if !state
        .calendar_events_resource
        .resolve(request, events.clone())
    {
        return false;
    }

    if let Some(selected_event) = state.selected_event.as_deref() {
        if !events.iter().any(|event| event.id == selected_event) {
            state.selected_event = None;
            clear_calendar_event_links(state);
        }
    }
    state.calendar_events = events;
    state.last_error = None;
    true
}

pub(crate) fn reject_calendar_events_load(
    state: &mut NativeAppState,
    request: ResourceRequest,
    err: anyhow::Error,
) -> bool {
    let message = err.to_string();
    if !state
        .calendar_events_resource
        .reject(request, message.clone())
    {
        return false;
    }

    state.calendar_events.clear();
    state.selected_event = None;
    clear_calendar_event_links(state);
    state.last_error = Some(message);
    true
}

pub(crate) fn apply_loaded_calendar_event_links(
    state: &mut NativeAppState,
    request: ResourceRequest,
    event_id: &str,
    links: Vec<EventLink>,
) -> bool {
    if state.selected_event.as_deref() != Some(event_id)
        || state.calendar_event_links_event.as_deref() != Some(event_id)
    {
        return false;
    }
    if !state
        .calendar_event_links_resource
        .resolve(request, links.clone())
    {
        return false;
    }

    state.calendar_event_links = links;
    state.last_error = None;
    true
}

pub(crate) fn reject_calendar_event_links_load(
    state: &mut NativeAppState,
    request: ResourceRequest,
    event_id: &str,
    err: anyhow::Error,
) -> bool {
    if state.selected_event.as_deref() != Some(event_id)
        || state.calendar_event_links_event.as_deref() != Some(event_id)
    {
        return false;
    }

    let message = err.to_string();
    if !state
        .calendar_event_links_resource
        .reject(request, message.clone())
    {
        return false;
    }

    state.last_error = Some(message);
    true
}

pub(crate) fn start_calendar_view(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    let should_load = state_entity.update(app_cx, |state, cx| {
        state.selected_view = SelectedView::Calendar;
        state.selected_message_ids.clear();
        let should_load = state.calendar_events_resource.is_idle()
            || state.calendar_events_resource.is_error()
            || state.calendar_events.is_empty();
        cx.notify();
        should_load
    });

    if should_load {
        start_calendar_events_load(state_entity, app_cx);
    }
}

pub(crate) fn shift_calendar_week(
    state_entity: &Entity<NativeAppState>,
    weeks: i64,
    app_cx: &mut App,
) {
    state_entity.update(app_cx, |state, cx| {
        state.calendar_week_start += ChronoDuration::days(weeks.saturating_mul(7));
        state.selected_view = SelectedView::Calendar;
        state.selected_event = None;
        clear_calendar_event_links(state);
        cx.notify();
    });
    start_calendar_events_load(state_entity, app_cx);
}

pub(crate) fn reset_calendar_week(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    state_entity.update(app_cx, |state, cx| {
        state.calendar_week_start = current_calendar_week_start();
        state.selected_view = SelectedView::Calendar;
        state.selected_event = None;
        clear_calendar_event_links(state);
        cx.notify();
    });
    start_calendar_events_load(state_entity, app_cx);
}

pub(crate) fn start_calendar_events_load(state_entity: &Entity<NativeAppState>, app_cx: &mut App) {
    let (base_url, request, from, to) = state_entity.update(app_cx, |state, cx| {
        let request = state.calendar_events_resource.begin_load();
        let (from, to) = calendar_week_bounds_utc(state.calendar_week_start);
        state.last_error = None;
        cx.notify();
        (state.api_base_url.clone(), request, from, to)
    });

    let task = app_cx
        .background_executor()
        .spawn(async move { load_calendar_events(base_url, from, to) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(events) => {
                            apply_loaded_calendar_events(state, request, events);
                        }
                        Err(err) => {
                            reject_calendar_events_load(state, request, err);
                        }
                    }
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native calendar events load: {err}");
                }
            }
        })
        .detach();
}

pub(crate) fn start_calendar_event_links_load(
    state_entity: &Entity<NativeAppState>,
    event_id: String,
    app_cx: &mut App,
) {
    let (base_url, request) = state_entity.update(app_cx, |state, cx| {
        state.calendar_event_links.clear();
        state.calendar_event_links_resource.reset();
        state.calendar_event_links_event = Some(event_id.clone());
        let request = state.calendar_event_links_resource.begin_load();
        state.last_error = None;
        cx.notify();
        (state.api_base_url.clone(), request)
    });

    let load_event_id = event_id.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { load_calendar_event_links(base_url, load_event_id) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(links) => {
                            apply_loaded_calendar_event_links(state, request, &event_id, links);
                        }
                        Err(err) => {
                            reject_calendar_event_links_load(state, request, &event_id, err);
                        }
                    }
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native calendar event links load: {err}");
                }
            }
        })
        .detach();
}

pub(crate) fn start_calendar_event_unlink(
    state_entity: &Entity<NativeAppState>,
    event_id: String,
    message_id: String,
    app_cx: &mut App,
) {
    let (base_url, request) = state_entity.update(app_cx, |state, cx| {
        state.calendar_event_links_event = Some(event_id.clone());
        let request = state.calendar_event_links_resource.begin_load();
        state.last_error = None;
        cx.notify();
        (state.api_base_url.clone(), request)
    });

    let task_event_id = event_id.clone();
    let task_message_id = message_id.clone();
    let task = app_cx
        .background_executor()
        .spawn(async move { remove_calendar_event_link(base_url, task_event_id, task_message_id) });
    let state_entity = state_entity.clone();
    app_cx
        .spawn(move |async_cx: &mut gpui::AsyncApp| {
            let mut async_cx = async_cx.clone();
            async move {
                let result = task.await;
                if let Err(err) = state_entity.update(&mut async_cx, |state, cx| {
                    match result {
                        Ok(links) => {
                            apply_loaded_calendar_event_links(state, request, &event_id, links);
                        }
                        Err(err) => {
                            reject_calendar_event_links_load(state, request, &event_id, err);
                        }
                    }
                    cx.notify();
                }) {
                    tracing::warn!("failed to apply native calendar event unlink: {err}");
                }
            }
        })
        .detach();
}

pub(crate) fn open_calendar_link_message(
    state_entity: &Entity<NativeAppState>,
    folder_id: String,
    message_id: String,
    app_cx: &mut App,
) {
    start_message_collection_load_with_preferred(
        state_entity,
        NativeMessageSource::Folder(folder_id),
        Some(message_id),
        app_cx,
    );
}

pub(crate) fn open_calendar_search_result(
    state_entity: &Entity<NativeAppState>,
    event_id: String,
    start_at: String,
    app_cx: &mut App,
) {
    let selected_event =
        state_entity.update(
            app_cx,
            |state, cx| match apply_calendar_search_result_selection(
                state,
                event_id.clone(),
                &start_at,
            ) {
                Ok(()) => {
                    cx.notify();
                    Some(event_id.clone())
                }
                Err(message) => {
                    state.last_error = Some(message);
                    cx.notify();
                    None
                }
            },
        );

    if let Some(event_id) = selected_event {
        start_calendar_events_load(state_entity, app_cx);
        start_calendar_event_links_load(state_entity, event_id, app_cx);
    }
}

pub(crate) fn apply_calendar_search_result_selection(
    state: &mut NativeAppState,
    event_id: String,
    start_at: &str,
) -> std::result::Result<(), String> {
    let week_start = calendar_week_start_from_iso(start_at)
        .ok_or_else(|| format!("Invalid calendar event start time: {start_at}"))?;
    state.selected_view = SelectedView::Calendar;
    state.calendar_week_start = week_start;
    state.selected_event = Some(event_id);
    state.selected_message_ids.clear();
    clear_calendar_event_links(state);
    state.last_error = None;
    Ok(())
}
