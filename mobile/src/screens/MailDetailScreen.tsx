import { useEffect, useState } from 'react';
import styles from './MailDetailScreen.module.css';
import { sanitizeEmailHtml } from '../utils/sanitizeEmailHtml';
import { useAppStore } from '../store';
import { api } from '../api/client';
import type { MessageBody } from '../types';

function formatDate(d: string | null) {
  if (!d) return '';
  return new Date(d).toLocaleString([], {
    weekday: 'short', month: 'short', day: 'numeric',
    hour: '2-digit', minute: '2-digit',
  });
}

function avatar(name: string) {
  const hue = name.split('').reduce((n, c) => n + c.charCodeAt(0), 0) % 360;
  return { bg: `oklch(62% 0.12 ${hue})`, initials: name.split(' ').map((w) => w[0]).join('').slice(0, 2).toUpperCase() };
}

export function MailDetailScreen() {
  const { selectedMessage, setSelectedMessage, theme } = useAppStore();
  const [body, setBody] = useState<MessageBody | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionError, setActionError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);

  useEffect(() => {
    if (!selectedMessage) return;
    setLoading(true);
    setBody(null);
    api.message(selectedMessage.id)
      .then((m) => { setBody(m as MessageBody); api.markRead(selectedMessage.id); })
      .catch(() => { /* ignore */ })
      .finally(() => setLoading(false));
  }, [selectedMessage?.id]);

  if (!selectedMessage) return null;

  const sender = body?.fromName ?? selectedMessage.fromName ?? 'Unknown';
  const av = avatar(sender);

  return (
    <div className={styles.screen}>
      {/* Header */}
      <header className={styles.header}>
        <button className={styles.back} onClick={() => setSelectedMessage(null)} aria-label="Back">
          <svg viewBox="0 0 10 16" fill="none" stroke="currentColor" strokeWidth="2" width="10" height="16" strokeLinecap="round" strokeLinejoin="round">
            <path d="M8 1L2 8l6 7" />
          </svg>
        </button>
        <div className={styles.headerActions}>
          <button className={styles.iconBtn} aria-label="Reply" onClick={() => useAppStore.getState().openCompose()}>
            <img src={theme === 'dark' ? '/icons/reply-dark.png' : '/icons/reply-light.png'} alt="" width="20" height="20" />
          </button>
          <button className={styles.iconBtn} aria-label="Star">
            <img src={theme === 'dark' ? '/icons/star-dark.png' : '/icons/star-light.png'} alt="" width="20" height="20" />
          </button>
          <button className={styles.iconBtn} aria-label="Trash" disabled={deleting} onClick={async () => {
            setDeleting(true);
            setActionError(null);
            try {
              await api.trash(selectedMessage.id);
              setSelectedMessage(null);
            } catch (cause) {
              setActionError(cause instanceof Error ? cause.message : 'Unable to delete message');
            } finally {
              setDeleting(false);
            }
          }}>
            <img src={theme === 'dark' ? '/icons/trash-dark.png' : '/icons/trash-light.png'} alt="" width="20" height="20" />
          </button>
        </div>
      </header>

      <div className={`${styles.body} scroll`}>
        {actionError && <div role="alert" style={{ color: 'var(--danger)', paddingBottom: 10 }}>{actionError}</div>}
        {/* Subject */}
        <h1 className={styles.subject}>
          {body?.subject ?? selectedMessage.subject ?? '(no subject)'}
        </h1>

        {/* Sender row */}
        <div className={styles.senderRow}>
          <div className={styles.avatar} style={{ background: av.bg }}>{av.initials}</div>
          <div className={styles.senderInfo}>
            <span className={styles.senderName}>{sender}</span>
            <span className={styles.senderEmail}>{body?.fromEmail ?? selectedMessage.fromEmail}</span>
          </div>
          <span className={styles.date}>{formatDate(body?.date ?? selectedMessage.date)}</span>
        </div>

        {/* Body */}
        {loading && <div className={styles.loading}>Loading…</div>}
        {!loading && body?.body?.htmlBody && (
          <iframe
            className={styles.iframe}
            srcDoc={sanitizeEmailHtml(body.body.htmlBody)}
            sandbox=""
            referrerPolicy="no-referrer"
            title="Email body"
          />
        )}
        {!loading && !body?.body?.htmlBody && body?.body?.textBody && (
          <pre className={styles.textBody}>{body.body.textBody}</pre>
        )}
        {!loading && !body?.body?.htmlBody && !body?.body?.textBody && (
          <div className={styles.loading}>No content</div>
        )}

        <div className={styles.bottomPad} />
      </div>
    </div>
  );
}
