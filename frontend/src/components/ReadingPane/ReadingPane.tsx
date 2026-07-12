import React, { useState, useCallback, useEffect, useRef } from 'react';
import styles from './ReadingPane.module.css';
import { useAppStore } from '../../store';
import { useApi } from '../../hooks/useApi';
import { useContextMenu } from '../ContextMenu/ContextMenu';
import type { Label } from '../../types';
import { sanitizeEmailHtml } from '../../utils/sanitizeEmailHtml';

interface FullMessage {
  id: string;
  accountId: string;
  subject: string | null;
  fromName: string | null;
  fromEmail: string | null;
  messageId: string | null;
  date: string | null;
  isRead: boolean;
  isFlagged: boolean;
  hasAttachments: boolean;
  body: {
    htmlBody: string | null;
    textBody: string | null;
  } | null;
  attachments: MessageAttachment[];
}

interface MessageAttachment {
  id: string;
  filename: string;
  contentType: string;
  sizeBytes: number;
}

function getAvatarStyle(name: string): { bg: string; initials: string } {
  const hue = name.split('').reduce((n, c) => n + c.charCodeAt(0), 0) % 360;
  return {
    bg: `oklch(62% 0.12 ${hue})`,
    initials: name.split(' ').map((w) => w[0]).join('').slice(0, 2).toUpperCase(),
  };
}

function formatFullDate(dateStr: string | null): string {
  if (!dateStr) return '';
  return new Date(dateStr).toLocaleString([], {
    weekday: 'short', year: 'numeric', month: 'short',
    day: 'numeric', hour: '2-digit', minute: '2-digit',
  });
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

const RP = { fill: 'none', stroke: 'currentColor', strokeWidth: 1.5, strokeLinecap: 'round' as const, strokeLinejoin: 'round' as const };

function ReplyIcon() {
  return (
    <svg viewBox="0 0 14 14" {...RP}>
      <path d="M2 7h7.5a3 3 0 010 6H8" />
      <path d="M2 7l3.5-3.5M2 7l3.5 3.5" />
    </svg>
  );
}

function ForwardIcon() {
  return (
    <svg viewBox="0 0 14 14" {...RP}>
      <path d="M12 7H4.5a3 3 0 000 6H6" />
      <path d="M12 7L8.5 3.5M12 7L8.5 10.5" />
    </svg>
  );
}

function AttachIcon() {
  return (
    <svg viewBox="0 0 14 14" {...RP}>
      <path d="M2.5 6.5L6.5 2.5a2.5 2.5 0 013.5 3.5L4.5 11a1.2 1.2 0 01-1.7-1.7L8 4" />
    </svg>
  );
}

function ArchiveIcon() {
  return (
    <svg viewBox="0 0 14 14" {...RP}>
      <rect x="1" y="1.5" width="12" height="3" rx="1" />
      <path d="M1.5 4.5v7a1 1 0 001 1h9a1 1 0 001-1v-7" />
      <path d="M5.5 8h3" />
    </svg>
  );
}

function TrashIcon() {
  return (
    <svg viewBox="0 0 14 14" {...RP}>
      <path d="M1.5 4h11" />
      <path d="M5.5 4V3a.5.5 0 01.5-.5h2a.5.5 0 01.5.5v1" />
      <path d="M3 4l.7 8a1 1 0 001 .9h5.6a1 1 0 001-.9L12 4" />
    </svg>
  );
}

function SnoozeIcon() {
  return (
    <svg viewBox="0 0 14 14" {...RP}>
      <circle cx="7" cy="7" r="5.5" />
      <path d="M7 4.5v3l1.5 1.5" />
    </svg>
  );
}

function snoozeUntilLaterToday(): string {
  const d = new Date();
  d.setHours(d.getHours() + 3, 0, 0, 0);
  return d.toISOString();
}

function snoozeUntilTomorrowMorning(): string {
  const d = new Date();
  d.setDate(d.getDate() + 1);
  d.setHours(9, 0, 0, 0);
  return d.toISOString();
}

function snoozeUntilNextWeek(): string {
  const d = new Date();
  const day = d.getDay();
  // Days until next Monday (1 = Monday)
  const daysUntilMonday = day === 0 ? 1 : 8 - day;
  d.setDate(d.getDate() + daysUntilMonday);
  d.setHours(9, 0, 0, 0);
  return d.toISOString();
}

function ReadIcon({ isRead }: { isRead: boolean }) {
  return isRead ? (
    <svg viewBox="0 0 14 14" {...RP}>
      <path d="M1.5 7.5l3.5 3.5 7.5-7.5" />
    </svg>
  ) : (
    <svg viewBox="0 0 14 14" {...RP}>
      <circle cx="11" cy="4" r="2" fill="var(--accent)" stroke="none" />
      <path d="M1.5 4.5h8M1.5 7.5h11M1.5 10.5h11" />
    </svg>
  );
}

export function ReadingPane() {
  const { selectedMessageId, setSelectedMessage, openCompose, theme, accounts, patchMessage } = useAppStore();
  const [replyText, setReplyText] = useState('');
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [localRead, setLocalRead] = useState<boolean | null>(null);
  const [snoozeOpen, setSnoozeOpen] = useState(false);
  const [customSnooze, setCustomSnooze] = useState('');
  const [showLabelDropdown, setShowLabelDropdown] = useState(false);
  const snoozeRef = useRef<HTMLDivElement>(null);
  const labelDropdownRef = useRef<HTMLDivElement>(null);
  const { contextMenu, openContextMenu } = useContextMenu();

  const { data: message, loading } = useApi<FullMessage>(
    selectedMessageId ? `/api/v1/messages/${selectedMessageId}` : '',
    { immediate: !!selectedMessageId },
  );

  const { data: messageLabels, refetch: refetchMessageLabels } = useApi<Label[]>(
    selectedMessageId ? `/api/v1/messages/${selectedMessageId}/labels` : '',
    { immediate: !!selectedMessageId },
  );

  const accountId = message?.accountId ?? accounts?.[0]?.id ?? '';
  const { data: allLabels } = useApi<Label[]>(
    accountId ? `/api/v1/labels?account_id=${accountId}` : '',
    { immediate: !!accountId },
  );

  // Reset local overrides when message changes.
  useEffect(() => { setLocalRead(null); setShowLabelDropdown(false); }, [selectedMessageId]);

  // Sync read state back to the message list when a message is opened.
  useEffect(() => {
    if (message?.id) patchMessage(message.id, { isRead: true });
  }, [message?.id, patchMessage]);

  const isRead = localRead ?? message?.isRead ?? true;

  const handleArchive = useCallback(async () => {
    if (!selectedMessageId) return;
    await fetch(`/api/v1/messages/${selectedMessageId}/archive`, { method: 'POST' });
    setSelectedMessage(null);
  }, [selectedMessageId, setSelectedMessage]);

  const handleDelete = useCallback(async () => {
    if (!selectedMessageId) return;
    await fetch(`/api/v1/messages/${selectedMessageId}`, { method: 'DELETE' });
    setSelectedMessage(null);
  }, [selectedMessageId, setSelectedMessage]);

  const handleToggleRead = useCallback(async () => {
    if (!selectedMessageId || !message) return;
    const newRead = !isRead;
    setLocalRead(newRead);
    await fetch(`/api/v1/messages/${selectedMessageId}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ isRead: newRead }),
    });
  }, [selectedMessageId, message, isRead]);

  const handleSnooze = useCallback(async (until: string) => {
    if (!selectedMessageId) return;
    setSnoozeOpen(false);
    await fetch(`/api/v1/messages/${selectedMessageId}/snooze`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ until }),
    });
    setSelectedMessage(null);
  }, [selectedMessageId, setSelectedMessage]);

  // Close snooze dropdown on outside click.
  useEffect(() => {
    if (!snoozeOpen) return;
    const onDown = (e: MouseEvent) => {
      if (snoozeRef.current && !snoozeRef.current.contains(e.target as Node)) {
        setSnoozeOpen(false);
      }
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [snoozeOpen]);

  // Close label dropdown on outside click.
  useEffect(() => {
    if (!showLabelDropdown) return;
    const onDown = (e: MouseEvent) => {
      if (labelDropdownRef.current && !labelDropdownRef.current.contains(e.target as Node)) {
        setShowLabelDropdown(false);
      }
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [showLabelDropdown]);

  const handleAddLabel = useCallback(async (labelId: string) => {
    if (!selectedMessageId) return;
    setShowLabelDropdown(false);
    await fetch(`/api/v1/messages/${selectedMessageId}/labels/${labelId}`, { method: 'POST' });
    refetchMessageLabels();
  }, [selectedMessageId, refetchMessageLabels]);

  const handleRemoveLabel = useCallback(async (labelId: string) => {
    if (!selectedMessageId) return;
    await fetch(`/api/v1/messages/${selectedMessageId}/labels/${labelId}`, { method: 'DELETE' });
    refetchMessageLabels();
  }, [selectedMessageId, refetchMessageLabels]);

  // Keyboard shortcuts: e=archive, #=delete, u=mark unread, Escape=close
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!selectedMessageId) return;
      const tag = (e.target as HTMLElement).tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA') return;
      if (e.key === 'e') handleArchive();
      if (e.key === '#') handleDelete();
      if (e.key === 'u') handleToggleRead();
      if (e.key === 'Escape') setSelectedMessage(null);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [selectedMessageId, handleArchive, handleDelete, handleToggleRead, setSelectedMessage]);

  const handleReply = useCallback(() => {
    if (!message) return;
    openCompose({
      accountId: message.accountId,
      to: message.fromEmail ?? '',
      subject: message.subject ? `Re: ${message.subject}` : 'Re:',
      inReplyTo: message.messageId ?? undefined,
      mode: 'reply',
      quotedText: message.body?.textBody ?? undefined,
      quotedFrom: message.fromName ?? message.fromEmail ?? undefined,
    });
  }, [message, openCompose]);

  const handleForward = useCallback(() => {
    if (!message) return;
    openCompose({
      accountId: message.accountId,
      to: '',
      subject: message.subject ? `Fwd: ${message.subject}` : 'Fwd:',
      mode: 'forward',
      quotedText: message.body?.textBody ?? undefined,
      quotedFrom: message.fromName ?? message.fromEmail ?? undefined,
    });
  }, [message, openCompose]);

  const handleSendReply = useCallback(async () => {
    if (!message || !replyText.trim()) return;
    setSending(true);
    setSendError(null);
    try {
      const resp = await fetch('/api/v1/messages', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          accountId: message.accountId,
          to: [message.fromEmail ?? ''],
          cc: [],
          bcc: [],
          subject: message.subject ? `Re: ${message.subject}` : 'Re:',
          textBody: replyText,
          inReplyTo: message.messageId,
        }),
      });
      if (!resp.ok) {
        const err = await resp.json().catch(() => ({ error: 'Send failed' }));
        setSendError(err.error ?? 'Send failed');
      } else {
        setReplyText('');
      }
    } catch (e) {
      setSendError('Network error');
    } finally {
      setSending(false);
    }
  }, [message, replyText]);

  if (!selectedMessageId) {
    return (
      <div className={styles.readingPane}>
        <div className={styles.empty}>Select a message to read</div>
      </div>
    );
  }

  if (loading) {
    return (
      <div className={styles.readingPane}>
        <div className={styles.empty}>Loading…</div>
      </div>
    );
  }

  if (!message) {
    return (
      <div className={styles.readingPane}>
        <div className={styles.empty}>Message not found</div>
      </div>
    );
  }

  const senderName = message.fromName ?? message.fromEmail ?? 'Unknown';
  const avatar = getAvatarStyle(senderName);

  return (
    <div className={styles.readingPane}>
      {/* Toolbar */}
      <div className={styles.toolbar}>
        <button className={styles.toolbarBtn} type="button" onClick={handleReply}>
          <ReplyIcon />
          Reply
        </button>
        <button className={styles.toolbarBtn} type="button" onClick={handleForward}>
          <ForwardIcon />
          Forward
        </button>
        <div className={styles.toolbarSpacer} />
        <button className={styles.toolbarBtn} type="button" onClick={handleToggleRead} title={isRead ? 'Mark unread (u)' : 'Mark read (u)'}>
          <ReadIcon isRead={isRead} />
          {isRead ? 'Unread' : 'Read'}
        </button>
        <div className={styles.snoozeWrapper} ref={snoozeRef}>
          <button
            className={styles.toolbarBtn}
            type="button"
            onClick={() => setSnoozeOpen((v) => !v)}
            title="Snooze"
          >
            <SnoozeIcon />
            Snooze
          </button>
          {snoozeOpen && (
            <div className={styles.snoozeDropdown}>
              <button
                type="button"
                className={styles.snoozeOption}
                onClick={() => handleSnooze(snoozeUntilLaterToday())}
              >
                Later today (3 hours)
              </button>
              <button
                type="button"
                className={styles.snoozeOption}
                onClick={() => handleSnooze(snoozeUntilTomorrowMorning())}
              >
                Tomorrow morning (9am)
              </button>
              <button
                type="button"
                className={styles.snoozeOption}
                onClick={() => handleSnooze(snoozeUntilNextWeek())}
              >
                Next week (Mon 9am)
              </button>
              <div className={styles.snoozeCustomRow}>
                <input
                  type="datetime-local"
                  className={styles.snoozeCustomInput}
                  value={customSnooze}
                  onChange={(e) => setCustomSnooze(e.target.value)}
                />
                <button
                  type="button"
                  className={styles.snoozeCustomBtn}
                  disabled={!customSnooze}
                  onClick={() => {
                    if (customSnooze) handleSnooze(new Date(customSnooze).toISOString());
                  }}
                >
                  Set
                </button>
              </div>
            </div>
          )}
        </div>
        <button className={styles.toolbarBtn} type="button" onClick={handleArchive} title="Archive (e)">
          <ArchiveIcon />
          Archive
        </button>
        <button className={`${styles.toolbarBtn} ${styles.toolbarBtnDanger}`} type="button" onClick={handleDelete} title="Delete (#)">
          <TrashIcon />
          Delete
        </button>
      </div>

      {/* Message header */}
      <div
        className={styles.messageHeader}
        onContextMenu={(e) => openContextMenu(e, [
          { label: 'Reply', action: handleReply },
          { label: 'Forward', action: handleForward },
          { separator: true },
          { label: 'Archive', action: handleArchive },
          { label: 'Delete', action: handleDelete },
          { separator: true },
          { label: 'Mark Read', action: () => {
            if (!selectedMessageId) return;
            fetch(`/api/v1/messages/${selectedMessageId}`, {
              method: 'PATCH',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ isRead: true }),
            }).catch(() => undefined);
            setLocalRead(true);
          }},
          { label: 'Mark Unread', action: () => {
            if (!selectedMessageId) return;
            fetch(`/api/v1/messages/${selectedMessageId}`, {
              method: 'PATCH',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ isRead: false }),
            }).catch(() => undefined);
            setLocalRead(false);
          }},
          { label: 'Copy sender email', action: () => {
            if (message?.fromEmail) {
              navigator.clipboard.writeText(message.fromEmail).catch(() => undefined);
            }
          }, disabled: !message?.fromEmail },
        ])}
      >
        <div className={styles.subject}>{message.subject ?? '(no subject)'}</div>

        {message.isFlagged && (
          <div className={styles.tags}>
            <span className={styles.tag}>Flagged</span>
          </div>
        )}

        {/* Label chips */}
        {((messageLabels && messageLabels.length > 0) || allLabels?.length) && (
          <div className={styles.labelRow}>
            {(messageLabels ?? []).map((lbl) => (
              <span key={lbl.id} className={styles.labelChip} style={{ '--label-color': lbl.color } as React.CSSProperties}>
                <span className={styles.labelDot} />
                {lbl.name}
                <button
                  type="button"
                  className={styles.labelRemove}
                  onClick={() => handleRemoveLabel(lbl.id)}
                  aria-label={`Remove label ${lbl.name}`}
                >
                  ×
                </button>
              </span>
            ))}
            {allLabels && allLabels.length > 0 && (
              <div className={styles.labelAddWrapper} ref={labelDropdownRef}>
                <button
                  type="button"
                  className={styles.labelAddBtn}
                  onClick={() => setShowLabelDropdown((v) => !v)}
                >
                  + Label
                </button>
                {showLabelDropdown && (
                  <div className={styles.labelDropdown}>
                    {allLabels
                      .filter((l) => !(messageLabels ?? []).some((ml) => ml.id === l.id))
                      .map((lbl) => (
                        <button
                          key={lbl.id}
                          type="button"
                          className={styles.labelDropdownItem}
                          onClick={() => handleAddLabel(lbl.id)}
                        >
                          <span className={styles.labelDot} style={{ '--label-color': lbl.color } as React.CSSProperties} />
                          {lbl.name}
                        </button>
                      ))}
                  </div>
                )}
              </div>
            )}
          </div>
        )}

        <div className={styles.senderRow}>
          <div className={styles.avatar} style={{ background: avatar.bg }}>
            {avatar.initials}
          </div>
          <div className={styles.senderMeta}>
            <div className={styles.senderName}>{senderName}</div>
            {message.fromEmail && message.fromName && (
              <div className={styles.senderEmail}>&lt;{message.fromEmail}&gt;</div>
            )}
          </div>
          <div className={styles.messageDate}>{formatFullDate(message.date)}</div>
        </div>
      </div>

      {/* Attachments */}
      {message.attachments.length > 0 && (
        <div className={styles.attachments}>
          {message.attachments.map((attachment) => (
            <a
              key={attachment.id}
              className={styles.attachment}
              href={`/api/v1/messages/${encodeURIComponent(message.id)}/attachments/${encodeURIComponent(attachment.id)}`}
              download={attachment.filename}
              title={`Download ${attachment.filename}`}
            >
              <AttachIcon />
              <span>{attachment.filename}</span>
              <span>{formatBytes(attachment.sizeBytes)}</span>
            </a>
          ))}
        </div>
      )}

      {/* Body */}
      <div className={styles.body}>
        {message.body?.htmlBody ? (
          <iframe
            className={styles.bodyFrame}
            srcDoc={sanitizeEmailHtml(message.body.htmlBody, theme === 'dark')}
            sandbox=""
            referrerPolicy="no-referrer"
            title="Message body"
          />
        ) : (
          <pre style={{ whiteSpace: 'pre-wrap', fontFamily: 'inherit', fontSize: '14px' }}>
            {message.body?.textBody ?? ''}
          </pre>
        )}
      </div>

      {contextMenu}

      {/* Quick reply */}
      <div className={styles.quickReply}>
        <textarea
          className={styles.replyArea}
          placeholder={`Reply to ${senderName}…`}
          value={replyText}
          onChange={(e) => setReplyText(e.target.value)}
        />
        {sendError && (
          <div style={{ color: 'var(--color-error, red)', fontSize: '12px', padding: '4px 8px' }}>
            {sendError}
          </div>
        )}
        <div className={styles.replyActions}>
          <button
            className={styles.sendBtn}
            type="button"
            onClick={handleSendReply}
            disabled={sending || !replyText.trim()}
          >
            {sending ? 'Sending…' : 'Send'}
          </button>
        </div>
      </div>
    </div>
  );
}
