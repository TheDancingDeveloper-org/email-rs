import { useEffect, useState } from 'react';
import { api, reportApiError } from '../../api/client';
import styles from './EventDetail.module.css';
import { useAppStore } from '../../store';
import type { CalendarEvent, EventLink } from '../../types';

interface Props {
  event: CalendarEvent;
  onClose: () => void;
}

function fmtDateTime(isoStr: string, isAllDay: boolean): string {
  const d = new Date(isoStr);
  if (isAllDay) {
    return d.toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric', year: 'numeric' });
  }
  return d.toLocaleString(undefined, { weekday: 'short', month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
}

function fmtLinkedDate(dateStr: string | null): string {
  if (!dateStr) return '';
  const d = new Date(dateStr);
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
}

function responseStatusLabel(status: string | null): string {
  switch (status) {
    case 'accepted': return '✓';
    case 'declined': return '✗';
    case 'tentative': return '?';
    default: return '–';
  }
}

export function EventDetail({ event, onClose }: Props) {
  const { navigateToMessage } = useAppStore();
  const [links, setLinks] = useState<EventLink[]>([]);
  const [linksLoading, setLinksLoading] = useState(true);

  useEffect(() => {
    setLinksLoading(true);
    api.get<EventLink[]>(`/api/v1/calendar/events/${event.id}/links`)
      .then((data) => { setLinks(data); setLinksLoading(false); })
      .catch((e) => { reportApiError(e); setLinks([]); setLinksLoading(false); });
  }, [event.id]);

  const handleUnlink = async (messageId: string) => {
    await api.delete(`/api/v1/calendar/events/${event.id}/links/${messageId}`);
    setLinks((prev) => prev.filter((l) => l.messageId !== messageId));
  };

  const sameDay = event.isAllDay
    ? event.startAt.slice(0, 10) === event.endAt.slice(0, 10)
    : new Date(event.startAt).toDateString() === new Date(event.endAt).toDateString();

  return (
    <div className={styles.panel}>
      {/* Header */}
      <div className={styles.header}>
        <h2 className={styles.title}>{event.title}</h2>
        <button type="button" className={styles.closeBtn} onClick={onClose} aria-label="Close">
          <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" width="12" height="12">
            <path d="M1 1l8 8M9 1L1 9" />
          </svg>
        </button>
      </div>

      <div className={styles.body}>
        {/* Time */}
        <div className={styles.section}>
          <div className={styles.metaRow}>
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" width="14" height="14" className={styles.metaIcon}>
              <circle cx="8" cy="8" r="6" />
              <path d="M8 5v3l2 1.5" />
            </svg>
            <div className={styles.metaText}>
              <span>{fmtDateTime(event.startAt, event.isAllDay)}</span>
              {!sameDay && (
                <>
                  <span className={styles.timeSep}>→</span>
                  <span>{fmtDateTime(event.endAt, event.isAllDay)}</span>
                </>
              )}
              {sameDay && !event.isAllDay && (
                <span className={styles.timeEnd}>
                  {' – '}
                  {new Date(event.endAt).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })}
                </span>
              )}
              {event.isAllDay && <span className={styles.allDayBadge}>All day</span>}
            </div>
          </div>
        </div>

        {/* Location */}
        {event.location && (
          <div className={styles.section}>
            <div className={styles.metaRow}>
              <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" width="14" height="14" className={styles.metaIcon}>
                <path d="M8 1.5a5 5 0 015 5c0 3.5-5 8-5 8s-5-4.5-5-8a5 5 0 015-5z" />
                <circle cx="8" cy="6.5" r="1.5" />
              </svg>
              <span className={styles.metaText}>{event.location}</span>
            </div>
          </div>
        )}

        {/* Meet link */}
        {event.meetLink && (
          <div className={styles.section}>
            <a
              href={event.meetLink}
              target="_blank"
              rel="noopener noreferrer"
              className={styles.meetBtn}
            >
              <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" width="13" height="13">
                <rect x="1" y="5" width="10" height="7" rx="1.5" />
                <path d="M11 8l4-2.5v5L11 8z" />
              </svg>
              Join meeting
            </a>
          </div>
        )}

        {/* Description */}
        {event.description && (
          <div className={styles.section}>
            <p className={styles.description}>{event.description}</p>
          </div>
        )}

        {/* Attendees */}
        {event.attendees.length > 0 && (
          <div className={styles.section}>
            <div className={styles.sectionLabel}>Attendees</div>
            <div className={styles.attendeeList}>
              {event.attendees.map((a, i) => (
                <div key={i} className={styles.attendeeRow}>
                  <span className={styles.responseStatus}>{responseStatusLabel(a.responseStatus)}</span>
                  <span className={styles.attendeeName}>{a.name || a.email}</span>
                  {a.name && <span className={styles.attendeeEmail}>{a.email}</span>}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Linked emails */}
        <div className={styles.section}>
          <div className={styles.sectionLabel}>Linked emails</div>
          {linksLoading ? (
            <span className={styles.dimText}>Loading…</span>
          ) : links.length === 0 ? (
            <span className={styles.dimText}>No linked emails</span>
          ) : (
            <div className={styles.linkList}>
              {links.map((link) => (
                <div key={link.id} className={styles.linkRow}>
                  <div className={styles.linkInfo} onClick={() => navigateToMessage(link.folderId, link.messageId)}>
                    <span className={styles.linkSubject}>{link.subject || '(no subject)'}</span>
                    <span className={styles.linkMeta}>
                      {link.fromName || link.fromEmail || 'Unknown'}
                      {link.date && <> · {fmtLinkedDate(link.date)}</>}
                    </span>
                  </div>
                  <button
                    type="button"
                    className={styles.unlinkBtn}
                    onClick={() => handleUnlink(link.messageId)}
                    title="Unlink"
                  >
                    <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" width="10" height="10">
                      <path d="M1 1l8 8M9 1L1 9" />
                    </svg>
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
