use crate::prelude::*;
use crate::*;

#[derive(Clone)]
pub(crate) struct CalendarPlaceholder {
    pub(crate) state: Entity<NativeAppState>,
    pub(crate) time_grid_scroll: ScrollHandle,
    pub(crate) scrolled_time_grid_week: Option<NaiveDate>,
}

impl CalendarPlaceholder {
    pub(crate) fn new(cx: &mut Context<Self>, state: Entity<NativeAppState>) -> Self {
        cx.observe_global::<Theme>(|_, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            time_grid_scroll: ScrollHandle::new(),
            scrolled_time_grid_week: None,
        }
    }
}

impl Render for CalendarPlaceholder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        if self.scrolled_time_grid_week != Some(state.calendar_week_start) {
            self.time_grid_scroll
                .scroll_to_top_of_item(CALENDAR_INITIAL_SCROLL_HOUR);
            self.scrolled_time_grid_week = Some(state.calendar_week_start);
        }
        calendar_placeholder(&self.state, &state, &self.time_grid_scroll, cx)
    }
}

pub(crate) fn calendar_search_results_section(
    state_entity: &Entity<NativeAppState>,
    events: &[CalendarSearchResult],
    cx: &mut Context<MessageListPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let mut section = div()
        .flex_none()
        .border_b_1()
        .border_color(colors.stroke_neutral_subtle)
        .bg(colors.surface)
        .child(
            div()
                .px(px(12.0))
                .pt(px(10.0))
                .pb(px(5.0))
                .text_size(px(typography.caption.size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.on_subtle)
                .child(format!(
                    "Calendar result{}",
                    if events.len() == 1 { "" } else { "s" }
                )),
        );

    for (idx, event) in events.iter().enumerate() {
        section = section.child(calendar_search_result_row(state_entity, idx, event, cx));
    }

    section
}

pub(crate) fn calendar_search_result_row(
    state_entity: &Entity<NativeAppState>,
    idx: usize,
    event: &CalendarSearchResult,
    cx: &mut Context<MessageListPlaceholder>,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let event_id = event.id.clone();
    let start_at = event.start_at.clone();
    let title = event.title.clone();
    let time = calendar_search_result_time_label(event);
    let location = event
        .location
        .as_deref()
        .map(str::trim)
        .filter(|location| !location.is_empty())
        .unwrap_or("Calendar event")
        .to_string();

    div()
        .id(("calendar-search-result-row", idx as u64))
        .mx(px(8.0))
        .mb(px(6.0))
        .px(px(10.0))
        .py(px(8.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(colors.stroke_neutral_subtle)
        .bg(colors.neutral)
        .cursor_pointer()
        .hover({
            let hover = colors.neutral_hover;
            move |style| style.bg(hover)
        })
        .on_click({
            let state_entity = state_entity.clone();
            move |_, _, app_cx| {
                open_calendar_search_result(
                    &state_entity,
                    event_id.clone(),
                    start_at.clone(),
                    app_cx,
                );
            }
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    svg()
                        .path("icons/list_tree.svg")
                        .size(px(14.0))
                        .text_color(colors.accent),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(typography.body.size))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors.on_neutral)
                                .child(title),
                        )
                        .child(
                            div()
                                .pt(px(2.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(typography.caption.size))
                                .text_color(colors.on_subtle)
                                .child(format!("{time} - {location}")),
                        ),
                ),
        )
        .into_any_element()
}

pub(crate) fn calendar_placeholder(
    state_entity: &Entity<NativeAppState>,
    state: &NativeAppState,
    time_grid_scroll: &ScrollHandle,
    cx: &mut Context<CalendarPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let days = calendar_week_days(state.calendar_week_start);
    let events = state.calendar_events.clone();
    let selected_event = state
        .selected_event
        .as_deref()
        .and_then(|event_id| events.iter().find(|event| event.id == event_id))
        .cloned();
    let status = calendar_status_text(state);

    let mut header = div()
        .px(px(20.0))
        .py(px(12.0))
        .border_b_1()
        .border_color(colors.stroke_neutral_subtle)
        .flex()
        .items_center()
        .gap(px(10.0))
        .child(
            div()
                .text_size(px(typography.subtitle.size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.on_neutral)
                .child("Calendar"),
        )
        .child(div().flex_1())
        .child(
            Button::new("calendar-prev-week")
                .label("Previous")
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .on_click({
                    let state_entity = state_entity.clone();
                    move |_, _, app_cx| shift_calendar_week(&state_entity, -1, app_cx)
                }),
        )
        .child(
            Button::new("calendar-today")
                .label("Today")
                .appearance(ButtonAppearance::Neutral)
                .size(ButtonSize::Compact)
                .on_click({
                    let state_entity = state_entity.clone();
                    move |_, _, app_cx| reset_calendar_week(&state_entity, app_cx)
                }),
        )
        .child(
            Button::new("calendar-next-week")
                .label("Next")
                .appearance(ButtonAppearance::Subtle)
                .size(ButtonSize::Compact)
                .on_click({
                    let state_entity = state_entity.clone();
                    move |_, _, app_cx| shift_calendar_week(&state_entity, 1, app_cx)
                }),
        )
        .child(
            div()
                .min_w(px(160.0))
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child(calendar_week_range_text(state.calendar_week_start)),
        );

    if let Some((message, is_error)) = status {
        header = header.child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(if is_error {
                    colors.status_error
                } else {
                    colors.on_subtle
                })
                .child(message),
        );
    }

    let mut content = div().flex_1().min_h_0().flex().bg(colors.surface).child(
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(calendar_day_header_row(&days, cx))
            .child(calendar_all_day_row(
                state_entity,
                &events,
                &days,
                state,
                cx,
            ))
            .child(calendar_time_grid(
                state_entity,
                &events,
                &days,
                state,
                time_grid_scroll,
                cx,
            )),
    );

    if let Some(event) = selected_event {
        content = content.child(calendar_event_detail(state_entity, &event, state, cx));
    }

    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(colors.surface)
        .child(header)
        .child(content)
}

pub(crate) fn calendar_day_header_row(
    days: &[NaiveDate],
    cx: &mut Context<CalendarPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let mut row = div()
        .h(px(52.0))
        .flex()
        .border_b_1()
        .border_color(colors.stroke_neutral_subtle)
        .bg(colors.surface)
        .child(div().w(px(72.0)).flex_none());

    for day in days {
        let today = *day == Local::now().date_naive();
        row = row.child(
            div()
                .flex_1()
                .min_w(px(88.0))
                .h_full()
                .px(px(8.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(2.0))
                .border_l_1()
                .border_color(colors.stroke_neutral_subtle)
                .child(
                    div()
                        .text_size(px(typography.caption.size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.on_subtle)
                        .child(day.format("%a").to_string()),
                )
                .child(
                    div()
                        .size(px(24.0))
                        .rounded(px(999.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(if today { colors.accent } else { colors.surface })
                        .text_size(px(typography.body.size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if today {
                            colors.on_accent
                        } else {
                            colors.on_neutral
                        })
                        .child(day.day().to_string()),
                ),
        );
    }

    row
}

pub(crate) fn calendar_all_day_row(
    state_entity: &Entity<NativeAppState>,
    events: &[CalendarEvent],
    days: &[NaiveDate],
    state: &NativeAppState,
    cx: &mut Context<CalendarPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let has_all_day = events.iter().any(|event| event.is_all_day);
    let mut row = div()
        .min_h(px(if has_all_day { 44.0 } else { 0.0 }))
        .flex()
        .border_b_1()
        .border_color(colors.stroke_neutral_subtle)
        .when(has_all_day, |row| {
            row.child(
                div()
                    .w(px(72.0))
                    .flex_none()
                    .px(px(8.0))
                    .py(px(8.0))
                    .text_size(px(typography.caption.size))
                    .text_color(colors.on_subtle)
                    .child("all day"),
            )
        });

    if !has_all_day {
        return row;
    }

    for day in days {
        let day_events = events
            .iter()
            .filter(|event| event.is_all_day && calendar_event_matches_day(event, *day))
            .cloned()
            .collect::<Vec<_>>();
        let mut cell = div()
            .flex_1()
            .min_w(px(88.0))
            .px(px(6.0))
            .py(px(6.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .border_l_1()
            .border_color(colors.stroke_neutral_subtle);

        for event in day_events {
            cell = cell.child(calendar_event_chip(
                state_entity,
                event,
                state.selected_event.as_deref(),
                true,
                cx,
            ));
        }
        row = row.child(cell);
    }

    row
}

pub(crate) fn calendar_time_grid(
    state_entity: &Entity<NativeAppState>,
    events: &[CalendarEvent],
    days: &[NaiveDate],
    state: &NativeAppState,
    scroll_handle: &ScrollHandle,
    cx: &mut Context<CalendarPlaceholder>,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let mut grid = div()
        .id("calendar-time-grid")
        .relative()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(scroll_handle)
        .flex()
        .flex_col();

    for hour in 0..24 {
        let mut row = div()
            .h(px(CALENDAR_HOUR_HEIGHT))
            .flex_none()
            .flex()
            .border_b_1()
            .border_color(colors.stroke_neutral_subtle)
            .child(
                div()
                    .w(px(72.0))
                    .flex_none()
                    .px(px(8.0))
                    .pt(px(6.0))
                    .text_size(px(typography.caption.size))
                    .text_color(colors.on_subtle)
                    .child(calendar_hour_label(hour)),
            );

        for _day in days {
            let cell = div()
                .flex_1()
                .min_w(px(88.0))
                .h_full()
                .border_l_1()
                .border_color(colors.stroke_neutral_subtle);
            row = row.child(cell);
        }

        grid = grid.child(row);
    }

    let mut event_layer = div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(CALENDAR_DAY_HEIGHT))
        .flex()
        .child(div().w(px(72.0)).flex_none());
    for day in days {
        let layouts = timed_event_layouts_for_day(events, *day);
        let mut column = div().relative().h_full().flex_1().min_w(px(88.0));
        for layout in layouts {
            column = column.child(calendar_timed_event_chip(
                state_entity,
                layout,
                state.selected_event.as_deref(),
                cx,
            ));
        }
        event_layer = event_layer.child(column);
    }
    grid = grid.child(event_layer);

    if events.is_empty() && !state.calendar_events_resource.is_loading() {
        grid = grid.child(
            div()
                .absolute()
                .top(px(14.0))
                .left(px(72.0))
                .right_0()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(typography.body.size))
                .text_color(colors.on_subtle)
                .child("No events this week."),
        );
    }

    grid.into_any_element()
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TimedEventLayout {
    pub(crate) event: CalendarEvent,
    pub(crate) day: NaiveDate,
    pub(crate) top: f32,
    pub(crate) height: f32,
    pub(crate) column: usize,
    pub(crate) column_count: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct TimedEventInterval {
    pub(crate) event: CalendarEvent,
    pub(crate) start_minute: i32,
    pub(crate) end_minute: i32,
}

pub(crate) fn timed_event_layouts_for_day(
    events: &[CalendarEvent],
    day: NaiveDate,
) -> Vec<TimedEventLayout> {
    let mut intervals = events
        .iter()
        .filter(|event| !event.is_all_day)
        .filter_map(|event| {
            timed_event_day_minutes(event, day).map(|(start_minute, end_minute)| {
                TimedEventInterval {
                    event: event.clone(),
                    start_minute,
                    end_minute,
                }
            })
        })
        .collect::<Vec<_>>();

    intervals.sort_by(|a, b| {
        a.start_minute
            .cmp(&b.start_minute)
            .then(a.end_minute.cmp(&b.end_minute))
            .then(a.event.id.cmp(&b.event.id))
    });

    let mut layouts = Vec::new();
    let mut cluster = Vec::new();
    let mut cluster_end = 0;
    for interval in intervals {
        if cluster.is_empty() || interval.start_minute < cluster_end {
            cluster_end = cluster_end.max(interval.end_minute);
            cluster.push(interval);
        } else {
            layouts.extend(layout_timed_event_cluster(
                day,
                std::mem::take(&mut cluster),
            ));
            cluster_end = interval.end_minute;
            cluster.push(interval);
        }
    }
    if !cluster.is_empty() {
        layouts.extend(layout_timed_event_cluster(day, cluster));
    }

    layouts.sort_by(|a, b| {
        a.top
            .partial_cmp(&b.top)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.column.cmp(&b.column))
            .then(a.event.id.cmp(&b.event.id))
    });
    layouts
}

pub(crate) fn layout_timed_event_cluster(
    day: NaiveDate,
    cluster: Vec<TimedEventInterval>,
) -> Vec<TimedEventLayout> {
    let mut column_end_minutes: Vec<i32> = Vec::new();
    let mut assigned = Vec::with_capacity(cluster.len());
    for interval in cluster {
        let column = column_end_minutes
            .iter()
            .position(|end_minute| *end_minute <= interval.start_minute)
            .unwrap_or_else(|| {
                column_end_minutes.push(0);
                column_end_minutes.len() - 1
            });
        column_end_minutes[column] = interval.end_minute;
        assigned.push((interval, column));
    }

    let column_count = column_end_minutes.len().max(1);
    assigned
        .into_iter()
        .map(|(interval, column)| {
            let top = (interval.start_minute as f32 / 60.0) * CALENDAR_HOUR_HEIGHT;
            let raw_height = ((interval.end_minute - interval.start_minute).max(1) as f32 / 60.0)
                * CALENDAR_HOUR_HEIGHT;
            let remaining = (CALENDAR_DAY_HEIGHT - top).max(1.0);
            let height = raw_height.max(MIN_TIMED_EVENT_HEIGHT).min(remaining);
            TimedEventLayout {
                event: interval.event,
                day,
                top,
                height,
                column,
                column_count,
            }
        })
        .collect()
}

pub(crate) fn calendar_timed_event_chip(
    state_entity: &Entity<NativeAppState>,
    layout: TimedEventLayout,
    selected_event_id: Option<&str>,
    cx: &mut Context<CalendarPlaceholder>,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let selected = selected_event_id == Some(layout.event.id.as_str());
    let event_id = layout.event.id.clone();
    let day = layout.day;
    let title = layout.event.title.clone();
    let time_label = calendar_event_time_label(&layout.event);
    let width = 1.0 / layout.column_count.max(1) as f32;
    let left = width * layout.column as f32;
    let show_time = layout.height >= 30.0;

    div()
        .id(SharedString::from(format!(
            "calendar-timed-event-{event_id}-{day}"
        )))
        .absolute()
        .top(px(layout.top))
        .left(relative(left))
        .w(relative(width))
        .h(px(layout.height))
        .px(px(3.0))
        .cursor_pointer()
        .on_click({
            let state_entity = state_entity.clone();
            move |_, _, app_cx| {
                toggle_calendar_event_selection(&state_entity, event_id.clone(), app_cx)
            }
        })
        .child(
            div()
                .size_full()
                .overflow_hidden()
                .px(px(7.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(if selected {
                    colors.stroke_accent
                } else {
                    colors.stroke_neutral_subtle
                })
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
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(theme.typography.caption.size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .when(show_time, |chip| {
                    chip.child(
                        div()
                            .pt(px(2.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(10.0))
                            .text_color(if selected {
                                colors.on_accent
                            } else {
                                colors.on_subtle
                            })
                            .child(time_label),
                    )
                }),
        )
        .into_any_element()
}

pub(crate) fn calendar_event_chip(
    state_entity: &Entity<NativeAppState>,
    event: CalendarEvent,
    selected_event_id: Option<&str>,
    compact: bool,
    cx: &mut Context<CalendarPlaceholder>,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let selected = selected_event_id == Some(event.id.as_str());
    let event_id = event.id.clone();
    let title = event.title.clone();
    let time_label = calendar_event_time_label(&event);
    div()
        .id(SharedString::from(format!(
            "calendar-event-chip-{event_id}"
        )))
        .min_h(px(if compact { 24.0 } else { 36.0 }))
        .px(px(7.0))
        .py(px(4.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(if selected {
            colors.stroke_accent
        } else {
            colors.stroke_neutral_subtle
        })
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
        .cursor_pointer()
        .on_click({
            let state_entity = state_entity.clone();
            move |_, _, app_cx| {
                toggle_calendar_event_selection(&state_entity, event_id.clone(), app_cx)
            }
        })
        .child(
            div()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(theme.typography.caption.size))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .when(!compact, |chip| {
            chip.child(
                div()
                    .pt(px(2.0))
                    .text_size(px(10.0))
                    .text_color(if selected {
                        colors.on_accent
                    } else {
                        colors.on_subtle
                    })
                    .child(time_label),
            )
        })
        .into_any_element()
}

pub(crate) fn calendar_event_detail(
    state_entity: &Entity<NativeAppState>,
    event: &CalendarEvent,
    state: &NativeAppState,
    cx: &mut Context<CalendarPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let mut panel = div()
        .w(px(340.0))
        .flex_none()
        .min_h_0()
        .border_l_1()
        .border_color(colors.stroke_neutral_subtle)
        .bg(colors.surface)
        .flex()
        .flex_col()
        .child(
            div()
                .px(px(16.0))
                .py(px(12.0))
                .border_b_1()
                .border_color(colors.stroke_neutral_subtle)
                .flex()
                .items_start()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(typography.subtitle.size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.on_neutral)
                        .child(event.title.clone()),
                )
                .child(
                    Button::new("calendar-detail-close")
                        .label("Close")
                        .appearance(ButtonAppearance::Subtle)
                        .size(ButtonSize::Compact)
                        .on_click({
                            let state_entity = state_entity.clone();
                            move |_, _, app_cx| {
                                state_entity.update(app_cx, |state, cx| {
                                    state.selected_event = None;
                                    clear_calendar_event_links(state);
                                    cx.notify();
                                });
                            }
                        }),
                ),
        )
        .child(
            div()
                .id("calendar-detail-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p(px(16.0))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(calendar_detail_row(
                    "Time",
                    calendar_event_detail_time(event),
                    cx,
                ))
                .when_some(event.location.clone(), |body, location| {
                    body.child(calendar_detail_row("Location", location, cx))
                })
                .when_some(event.meet_link.clone(), |body, meet_link| {
                    body.child(
                        Button::new("calendar-join-meeting")
                            .label("Join meeting")
                            .appearance(ButtonAppearance::Accent)
                            .size(ButtonSize::Compact)
                            .on_click({
                                let state_entity = state_entity.clone();
                                move |_, _, app_cx| {
                                    open_event_meet_link(&state_entity, meet_link.clone(), app_cx);
                                }
                            }),
                    )
                })
                .when_some(event.description.clone(), |body, description| {
                    body.child(calendar_detail_row("Description", description, cx))
                })
                .child(calendar_event_links_section(state_entity, event, state, cx)),
        );

    if !event.attendees.is_empty() {
        let mut attendees = div().flex().flex_col().gap(px(4.0)).child(
            div()
                .text_size(px(typography.caption.size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.on_subtle)
                .child("Attendees"),
        );
        for attendee in &event.attendees {
            attendees = attendees.child(
                div()
                    .text_size(px(typography.body.size))
                    .text_color(colors.on_neutral)
                    .child(calendar_attendee_label(attendee)),
            );
        }
        panel = panel.child(
            div()
                .px(px(16.0))
                .pb(px(16.0))
                .border_t_1()
                .border_color(colors.stroke_neutral_subtle)
                .child(attendees),
        );
    }

    panel
}

pub(crate) fn calendar_event_links_section(
    state_entity: &Entity<NativeAppState>,
    event: &CalendarEvent,
    state: &NativeAppState,
    cx: &mut Context<CalendarPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    let event_id = event.id.clone();
    let links_match = state.calendar_event_links_event.as_deref() == Some(event.id.as_str());
    let loading = links_match && state.calendar_event_links_resource.is_loading();
    let error = links_match
        .then(|| state.calendar_event_links_resource.error().cloned())
        .flatten();
    let links = if links_match {
        state.calendar_event_links.clone()
    } else {
        Vec::new()
    };

    let mut section = div().flex().flex_col().gap(px(7.0)).child(
        div()
            .text_size(px(typography.caption.size))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(colors.on_subtle)
            .child("Linked emails"),
    );

    if links.is_empty() {
        let text = if loading {
            "Loading..."
        } else if error.is_some() {
            "Linked emails failed to load."
        } else {
            "No linked emails"
        };
        return section.child(
            div()
                .text_size(px(typography.body.size))
                .text_color(if error.is_some() {
                    colors.status_error
                } else {
                    colors.on_subtle
                })
                .child(text),
        );
    }

    for link in links {
        let link_id = link.id.clone();
        let message_id = link.message_id.clone();
        let unlink_message_id = link.message_id.clone();
        let folder_id = link.folder_id.clone();
        let subject = calendar_link_subject(&link);
        let meta = calendar_link_meta(&link);
        section = section.child(
            div()
                .id(SharedString::from(format!("calendar-link-row-{link_id}")))
                .px(px(8.0))
                .py(px(7.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(colors.stroke_neutral_subtle)
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .id(SharedString::from(format!("calendar-link-open-{link_id}")))
                        .flex_1()
                        .min_w_0()
                        .cursor_pointer()
                        .on_click({
                            let state_entity = state_entity.clone();
                            move |_, _, app_cx| {
                                open_calendar_link_message(
                                    &state_entity,
                                    folder_id.clone(),
                                    message_id.clone(),
                                    app_cx,
                                );
                            }
                        })
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(typography.body.size))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors.on_neutral)
                                .child(subject),
                        )
                        .child(
                            div()
                                .pt(px(2.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(typography.caption.size))
                                .text_color(colors.on_subtle)
                                .child(meta),
                        ),
                )
                .child(
                    Button::new(SharedString::from(format!(
                        "calendar-link-unlink-{link_id}"
                    )))
                    .label("Unlink")
                    .appearance(ButtonAppearance::Subtle)
                    .size(ButtonSize::Compact)
                    .disabled(loading)
                    .on_click({
                        let state_entity = state_entity.clone();
                        let event_id = event_id.clone();
                        move |_, _, app_cx| {
                            start_calendar_event_unlink(
                                &state_entity,
                                event_id.clone(),
                                unlink_message_id.clone(),
                                app_cx,
                            );
                        }
                    }),
                ),
        );
    }

    if loading {
        section = section.child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(colors.on_subtle)
                .child("Updating linked emails..."),
        );
    } else if let Some(error) = error {
        section = section.child(
            div()
                .text_size(px(typography.caption.size))
                .text_color(colors.status_error)
                .child(format!("Linked emails failed to update: {error}")),
        );
    }

    section
}

pub(crate) fn calendar_detail_row(
    label: &'static str,
    value: String,
    cx: &mut Context<CalendarPlaceholder>,
) -> gpui::Div {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let typography = theme.typography;
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(typography.caption.size))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.on_subtle)
                .child(label),
        )
        .child(
            div()
                .text_size(px(typography.body.size))
                .line_height(px(typography.body.line_height))
                .text_color(colors.on_neutral)
                .child(value),
        )
}

pub(crate) fn current_calendar_week_start() -> NaiveDate {
    monday_of_date(Local::now().date_naive())
}

pub(crate) fn monday_of_date(date: NaiveDate) -> NaiveDate {
    date - ChronoDuration::days(date.weekday().num_days_from_monday() as i64)
}

pub(crate) fn calendar_week_days(week_start: NaiveDate) -> Vec<NaiveDate> {
    (0..7)
        .map(|offset| week_start + ChronoDuration::days(offset))
        .collect()
}

pub(crate) fn calendar_week_start_from_iso(value: &str) -> Option<NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| monday_of_date(parsed.with_timezone(&Local).date_naive()))
}

pub(crate) fn calendar_week_bounds_utc(
    week_start: NaiveDate,
) -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
    let start = Local
        .with_ymd_and_hms(
            week_start.year(),
            week_start.month(),
            week_start.day(),
            0,
            0,
            0,
        )
        .single()
        .unwrap_or_else(Local::now);
    let end = start + ChronoDuration::days(7);
    (start.with_timezone(&Utc), end.with_timezone(&Utc))
}

pub(crate) fn calendar_week_range_text(week_start: NaiveDate) -> String {
    let end = week_start + ChronoDuration::days(6);
    let start_text = format!("{} {}", week_start.format("%b"), week_start.day());
    if week_start.year() == end.year() {
        let end_text = if week_start.month() == end.month() {
            end.day().to_string()
        } else {
            format!("{} {}", end.format("%b"), end.day())
        };
        format!("{start_text} - {end_text}, {}", end.year())
    } else {
        format!(
            "{start_text}, {} - {} {}, {}",
            week_start.year(),
            end.format("%b"),
            end.day(),
            end.year()
        )
    }
}

pub(crate) fn calendar_status_text(state: &NativeAppState) -> Option<(String, bool)> {
    if state.calendar_events_resource.is_loading() {
        return Some(("Loading events...".to_string(), false));
    }
    state
        .calendar_events_resource
        .error()
        .map(|error| (format!("Events failed to load: {error}"), true))
}

pub(crate) fn calendar_event_matches_day(event: &CalendarEvent, day: NaiveDate) -> bool {
    calendar_event_overlaps_day(event, day)
}

pub(crate) fn calendar_event_overlaps_day(event: &CalendarEvent, day: NaiveDate) -> bool {
    let Some((day_start, day_end)) = calendar_local_day_bounds(day) else {
        return event.start_at.with_timezone(&Local).date_naive() == day;
    };
    let (start, end) = calendar_event_local_interval(event);
    end > day_start && start < day_end
}

pub(crate) fn calendar_event_local_interval(
    event: &CalendarEvent,
) -> (chrono::DateTime<Local>, chrono::DateTime<Local>) {
    let start = event.start_at.with_timezone(&Local);
    let mut end = event.end_at.with_timezone(&Local);
    if end <= start {
        end = start
            + if event.is_all_day {
                ChronoDuration::days(1)
            } else {
                ChronoDuration::minutes(30)
            };
    }
    (start, end)
}

pub(crate) fn calendar_local_day_bounds(
    day: NaiveDate,
) -> Option<(chrono::DateTime<Local>, chrono::DateTime<Local>)> {
    let start = Local
        .with_ymd_and_hms(day.year(), day.month(), day.day(), 0, 0, 0)
        .earliest()?;
    Some((start, start + ChronoDuration::days(1)))
}

pub(crate) fn timed_event_day_minutes(event: &CalendarEvent, day: NaiveDate) -> Option<(i32, i32)> {
    let (day_start, day_end) = calendar_local_day_bounds(day)?;
    let (start, end) = calendar_event_local_interval(event);
    if end <= day_start || start >= day_end {
        return None;
    }

    let clipped_start = if start < day_start { day_start } else { start };
    let clipped_end = if end > day_end { day_end } else { end };
    let start_minute = if clipped_start <= day_start {
        0
    } else {
        (clipped_start.hour() * 60 + clipped_start.minute()) as i32
    };
    let mut end_minute = if clipped_end >= day_end {
        24 * 60
    } else {
        (clipped_end.hour() * 60 + clipped_end.minute()) as i32
    };
    if end_minute <= start_minute {
        end_minute = (start_minute + 30).min(24 * 60);
    }
    Some((start_minute, end_minute))
}

pub(crate) fn calendar_hour_label(hour: u32) -> String {
    match hour {
        0 => "12 AM".to_string(),
        1..=11 => format!("{hour} AM"),
        12 => "12 PM".to_string(),
        _ => format!("{} PM", hour - 12),
    }
}

pub(crate) fn calendar_time_label(value: &chrono::DateTime<Utc>) -> String {
    value.with_timezone(&Local).format("%-I:%M %p").to_string()
}

pub(crate) fn compact_date_label(value: Option<&str>) -> Option<String> {
    let value = value.map(str::trim).filter(|value| !value.is_empty())?;
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Local).format("%b %-d").to_string())
        .ok()
        .or_else(|| Some(value.to_string()))
}

pub(crate) fn calendar_search_result_time_label(event: &CalendarSearchResult) -> String {
    let parsed = chrono::DateTime::parse_from_rfc3339(&event.start_at);
    match parsed {
        Ok(start) if event.is_all_day => {
            start.with_timezone(&Local).format("%b %-d, %Y").to_string()
        }
        Ok(start) => start
            .with_timezone(&Local)
            .format("%b %-d, %-I:%M %p")
            .to_string(),
        Err(_) => event.start_at.clone(),
    }
}

pub(crate) fn calendar_event_time_label(event: &CalendarEvent) -> String {
    if event.is_all_day {
        "All day".to_string()
    } else {
        format!(
            "{} - {}",
            calendar_time_label(&event.start_at),
            calendar_time_label(&event.end_at)
        )
    }
}

pub(crate) fn calendar_event_detail_time(event: &CalendarEvent) -> String {
    if event.is_all_day {
        return event
            .start_at
            .with_timezone(&Local)
            .format("%A, %B %-d, %Y")
            .to_string();
    }

    let start = event.start_at.with_timezone(&Local);
    let end = event.end_at.with_timezone(&Local);
    if start.date_naive() == end.date_naive() {
        format!(
            "{} - {}",
            start.format("%a, %b %-d, %-I:%M %p"),
            end.format("%-I:%M %p")
        )
    } else {
        format!(
            "{} - {}",
            start.format("%a, %b %-d, %-I:%M %p"),
            end.format("%a, %b %-d, %-I:%M %p")
        )
    }
}

pub(crate) fn calendar_attendee_label(attendee: &email_types::Attendee) -> String {
    let name = attendee
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(attendee.email.as_str());
    let status = attendee
        .response_status
        .as_deref()
        .map(str::trim)
        .filter(|status| !status.is_empty())
        .unwrap_or("needs action");
    if name == attendee.email {
        format!("{name} - {status}")
    } else {
        format!("{name} <{}> - {status}", attendee.email)
    }
}

pub(crate) fn toggle_calendar_event_selection(
    state_entity: &Entity<NativeAppState>,
    event_id: String,
    app_cx: &mut App,
) {
    let selected_event = state_entity.update(app_cx, |state, cx| {
        let selected_event = if state.selected_event.as_deref() == Some(event_id.as_str()) {
            None
        } else {
            Some(event_id.clone())
        };
        state.selected_event = selected_event.clone();
        clear_calendar_event_links(state);
        cx.notify();
        selected_event
    });

    if let Some(event_id) = selected_event {
        start_calendar_event_links_load(state_entity, event_id, app_cx);
    }
}

pub(crate) fn clear_calendar_event_links(state: &mut NativeAppState) {
    state.calendar_event_links_event = None;
    state.calendar_event_links.clear();
    state.calendar_event_links_resource.reset();
}

pub(crate) fn calendar_link_sender(link: &EventLink) -> String {
    link.from_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            link.from_email
                .as_deref()
                .map(str::trim)
                .filter(|email| !email.is_empty())
        })
        .unwrap_or("Unknown")
        .to_string()
}

pub(crate) fn calendar_link_date_label(date: Option<&str>) -> Option<String> {
    let date = date.map(str::trim).filter(|date| !date.is_empty())?;
    chrono::DateTime::parse_from_rfc3339(date)
        .map(|parsed| {
            parsed
                .with_timezone(&Local)
                .format("%b %-d, %Y")
                .to_string()
        })
        .ok()
        .or_else(|| Some(date.to_string()))
}

pub(crate) fn calendar_link_meta(link: &EventLink) -> String {
    let sender = calendar_link_sender(link);
    if let Some(date) = calendar_link_date_label(link.date.as_deref()) {
        format!("{sender} - {date}")
    } else {
        sender
    }
}

pub(crate) fn calendar_link_subject(link: &EventLink) -> String {
    link.subject
        .as_deref()
        .map(str::trim)
        .filter(|subject| !subject.is_empty())
        .unwrap_or("(no subject)")
        .to_string()
}

pub(crate) fn open_event_meet_link(
    state_entity: &Entity<NativeAppState>,
    url: String,
    app_cx: &mut App,
) {
    let result = open_url_in_browser(&url);
    state_entity.update(app_cx, |state, cx| {
        match result {
            Ok(_) => state.last_error = None,
            Err(err) => state.last_error = Some(err.to_string()),
        }
        cx.notify();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_helpers_start_on_monday() {
        let wednesday = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let monday = monday_of_date(wednesday);
        assert_eq!(monday, NaiveDate::from_ymd_opt(2026, 11, 2).unwrap());
        assert_eq!(monday_of_date(monday), monday);
        let days = calendar_week_days(monday);
        assert_eq!(days.len(), 7);
        assert_eq!(days[6], NaiveDate::from_ymd_opt(2026, 11, 8).unwrap());
    }
}
