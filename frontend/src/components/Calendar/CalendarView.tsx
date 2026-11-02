import { useEffect, useMemo, useRef, useState } from 'react';
import { api, reportApiError } from '../../api/client';
import styles from './CalendarView.module.css';
import { useAppStore } from '../../store';
import { EventDetail } from './EventDetail';
import type { CalendarEvent } from '../../types';

const HOUR_PX = 48;
const TOTAL_PX = HOUR_PX * 24;

function mondayOf(d: Date): Date {
  const result = new Date(d);
  result.setHours(0, 0, 0, 0);
  const day = result.getDay();
  const diff = day === 0 ? -6 : 1 - day;
  result.setDate(result.getDate() + diff);
  return result;
}

function addDays(d: Date, n: number): Date {
  const r = new Date(d);
  r.setDate(r.getDate() + n);
  return r;
}

function isSameLocalDay(isoStr: string, day: Date): boolean {
  const d = new Date(isoStr);
  return (
    d.getFullYear() === day.getFullYear() &&
    d.getMonth() === day.getMonth() &&
    d.getDate() === day.getDate()
  );
}

function eventTop(isoStr: string): number {
  const d = new Date(isoStr);
  return (d.getHours() + d.getMinutes() / 60) * HOUR_PX;
}

function eventHeight(startIso: string, endIso: string): number {
  const start = new Date(startIso);
  const end = new Date(endIso);
  const hours = (end.getTime() - start.getTime()) / 3_600_000;
  return Math.max(20, hours * HOUR_PX);
}

function eventColor(calendarId: string): string {
  const hue = calendarId.split('').reduce((n, c) => n + c.charCodeAt(0), 0) % 360;
  return `oklch(50% 0.14 ${hue})`;
}

function fmtWeekRange(start: Date): string {
  const end = addDays(start, 6);
  const sameMonth = start.getMonth() === end.getMonth();
  const sameYear = start.getFullYear() === end.getFullYear();
  const opts: Intl.DateTimeFormatOptions = { month: 'short', day: 'numeric' };
  const startStr = start.toLocaleDateString(undefined, opts);
  const endOpts: Intl.DateTimeFormatOptions = sameMonth
    ? { day: 'numeric' }
    : sameYear
      ? { month: 'short', day: 'numeric' }
      : { month: 'short', day: 'numeric', year: 'numeric' };
  const endStr = end.toLocaleDateString(undefined, endOpts);
  const year = sameYear ? `, ${end.getFullYear()}` : '';
  return `${startStr} – ${endStr}${year}`;
}

function fmtDayHeader(day: Date): { weekday: string; date: string; isToday: boolean } {
  const today = new Date();
  const isToday =
    day.getFullYear() === today.getFullYear() &&
    day.getMonth() === today.getMonth() &&
    day.getDate() === today.getDate();
  return {
    weekday: day.toLocaleDateString(undefined, { weekday: 'short' }),
    date: String(day.getDate()),
    isToday,
  };
}

function fmtEventTime(isoStr: string): string {
  const d = new Date(isoStr);
  return d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
}

const HOUR_LABELS = Array.from({ length: 24 }, (_, i) => {
  if (i === 0) return '12 AM';
  if (i < 12) return `${i} AM`;
  if (i === 12) return '12 PM';
  return `${i - 12} PM`;
});

export function CalendarView() {
  const { selectedCalendarEventId, setSelectedCalendarEvent } = useAppStore();
  const [weekStart, setWeekStart] = useState(() => mondayOf(new Date()));
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);

  const weekDays = useMemo(() => Array.from({ length: 7 }, (_, i) => addDays(weekStart, i)), [weekStart]);

  const weekEnd = useMemo(() => addDays(weekStart, 7), [weekStart]);

  useEffect(() => {
    const from = weekStart.toISOString();
    const to = weekEnd.toISOString();
    api.get<CalendarEvent[]>(`/api/v1/calendar/events?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`)
      .then(setEvents)
      .catch((e) => { reportApiError(e); setEvents([]); });
  }, [weekStart, weekEnd]);

  // Scroll to 8am on mount
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = 8 * HOUR_PX - 16;
    }
  }, []);

  const goToday = () => setWeekStart(mondayOf(new Date()));
  const goPrev = () => setWeekStart((w) => addDays(w, -7));
  const goNext = () => setWeekStart((w) => addDays(w, 7));

  const timedEvents = events.filter((e) => !e.isAllDay);
  const allDayEvents = events.filter((e) => e.isAllDay);

  const selectedEvent = selectedCalendarEventId
    ? events.find((e) => e.id === selectedCalendarEventId) ?? null
    : null;

  return (
    <div className={styles.container}>
      {/* Toolbar */}
      <div className={styles.toolbar}>
        <span className={styles.toolbarTitle}>Calendar</span>
        <div className={styles.toolbarNav}>
          <button type="button" className={styles.navBtn} onClick={goPrev} aria-label="Previous week">
            <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" width="10" height="10">
              <path d="M7 2L4 5l3 3" />
            </svg>
          </button>
          <button type="button" className={styles.todayBtn} onClick={goToday}>Today</button>
          <button type="button" className={styles.navBtn} onClick={goNext} aria-label="Next week">
            <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" width="10" height="10">
              <path d="M3 2l3 3-3 3" />
            </svg>
          </button>
        </div>
        <span className={styles.weekLabel}>{fmtWeekRange(weekStart)}</span>
      </div>

      {/* Content */}
      <div className={styles.content}>
        <div className={styles.gridWrap}>
          {/* Day header row */}
          <div className={styles.dayHeaderRow}>
            <div className={styles.gutterCell} />
            {weekDays.map((day, i) => {
              const { weekday, date, isToday } = fmtDayHeader(day);
              return (
                <div key={i} className={`${styles.dayHeader}${isToday ? ` ${styles.dayHeaderToday}` : ''}`}>
                  <span className={styles.dayHeaderWeekday}>{weekday}</span>
                  <span className={`${styles.dayHeaderDate}${isToday ? ` ${styles.dayHeaderDateToday}` : ''}`}>
                    {date}
                  </span>
                </div>
              );
            })}
          </div>

          {/* All-day row */}
          {allDayEvents.length > 0 && (
            <div className={styles.allDayRow}>
              <div className={styles.allDayGutter}>all day</div>
              {weekDays.map((day, i) => {
                const dayEvents = allDayEvents.filter((e) => isSameLocalDay(e.startAt, day));
                return (
                  <div key={i} className={styles.allDayCell}>
                    {dayEvents.map((ev) => (
                      <div
                        key={ev.id}
                        className={`${styles.allDayChip}${ev.id === selectedCalendarEventId ? ` ${styles.chipSelected}` : ''}`}
                        style={{ background: eventColor(ev.calendarId) }}
                        onClick={() => setSelectedCalendarEvent(ev.id === selectedCalendarEventId ? null : ev.id)}
                      >
                        {ev.title}
                      </div>
                    ))}
                  </div>
                );
              })}
            </div>
          )}

          {/* Scrollable time grid */}
          <div className={styles.scrollArea} ref={scrollRef}>
            <div className={styles.timeGrid} style={{ height: TOTAL_PX }}>
              {/* Hour lines drawn as background on each column */}
              <div className={styles.timeGutter}>
                {HOUR_LABELS.map((label, h) => (
                  <div
                    key={h}
                    className={styles.hourLabel}
                    style={{ top: h * HOUR_PX - 8 }}
                  >
                    {label}
                  </div>
                ))}
              </div>

              {weekDays.map((day, di) => {
                const dayEvts = timedEvents.filter((e) => isSameLocalDay(e.startAt, day));
                return (
                  <div key={di} className={styles.dayColumn}>
                    {/* Hour grid lines */}
                    {Array.from({ length: 24 }, (_, h) => (
                      <div
                        key={h}
                        className={h % 2 === 0 ? styles.hourLine : styles.halfHourLine}
                        style={{ top: h * HOUR_PX }}
                      />
                    ))}
                    {/* Events */}
                    {dayEvts.map((ev) => {
                      const top = eventTop(ev.startAt);
                      const height = eventHeight(ev.startAt, ev.endAt);
                      const color = eventColor(ev.calendarId);
                      const isSelected = ev.id === selectedCalendarEventId;
                      return (
                        <div
                          key={ev.id}
                          className={`${styles.eventChip}${isSelected ? ` ${styles.chipSelected}` : ''}`}
                          style={{ top, height, background: color }}
                          onClick={() => setSelectedCalendarEvent(ev.id === selectedCalendarEventId ? null : ev.id)}
                          title={ev.title}
                        >
                          <span className={styles.chipTitle}>{ev.title}</span>
                          {height >= 36 && (
                            <span className={styles.chipTime}>{fmtEventTime(ev.startAt)}</span>
                          )}
                        </div>
                      );
                    })}
                  </div>
                );
              })}
            </div>
          </div>
        </div>

        {/* Event detail panel */}
        {selectedEvent && (
          <EventDetail
            event={selectedEvent}
            onClose={() => setSelectedCalendarEvent(null)}
          />
        )}
      </div>
    </div>
  );
}
