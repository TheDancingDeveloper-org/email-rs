import { useState, useEffect, useRef, useMemo } from 'react';
import { api } from '../../api/client';
import styles from './CommandPalette.module.css';
import { useAppStore } from '../../store';

type PaletteAction = {
  id: string;
  label: string;
  hint?: string;
  group?: string;
  keywords: string[];
  handler: () => void;
};

function matchScore(action: PaletteAction, query: string): number {
  const q = query.toLowerCase();
  const label = action.label.toLowerCase();
  if (label === q) return 100;
  if (label.startsWith(q)) return 80;
  if (label.includes(q)) return 60;
  if (action.keywords.some((k) => k.toLowerCase().includes(q))) return 40;
  // fuzzy: all chars of query appear in label in order
  let pos = 0;
  for (const ch of q) {
    const found = label.indexOf(ch, pos);
    if (found === -1) return 0;
    pos = found + 1;
  }
  return 20;
}

export function CommandPalette() {
  const {
    paletteOpen, closePalette,
    folders, accounts, labels,
    selectedMessageId, messages,
    setSelectedFolder, setSelectedLabel, openCompose, openSettings,
    setTheme, setDensity, theme, densityLevel,
    openAdvancedSearch,
    setView, view,
    patchMessage, removeMessage,
  } = useAppStore();

  const [query, setQuery] = useState('');
  const [activeIdx, setActiveIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (paletteOpen) {
      setQuery('');
      setActiveIdx(0);
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [paletteOpen]);

  const selectedMessage = messages.find((m) => m.id === selectedMessageId);

  const actions = useMemo<PaletteAction[]>(() => {
    const list: PaletteAction[] = [];

    // ── Actions ──────────────────────────────────────────────────────────────
    list.push({
      id: 'compose',
      label: 'New Message',
      hint: 'c',
      group: 'Actions',
      keywords: ['compose', 'write', 'email', 'new'],
      handler: () => {
        if (accounts[0]) openCompose({ accountId: accounts[0].id, to: '', subject: '', mode: 'compose' });
      },
    });

    list.push({
      id: 'advanced-search',
      label: 'Advanced Search',
      group: 'Actions',
      keywords: ['search', 'filter', 'find', 'advanced', 'query'],
      handler: openAdvancedSearch,
    });

    list.push({
      id: 'settings',
      label: 'Open Settings',
      group: 'Actions',
      keywords: ['settings', 'preferences', 'config', 'account', 'options'],
      handler: openSettings,
    });

    list.push({
      id: 'theme-toggle',
      label: theme === 'dark' ? 'Switch to Light Mode' : 'Switch to Dark Mode',
      group: 'Actions',
      keywords: ['theme', 'dark', 'light', 'mode', 'appearance', 'color'],
      handler: () => setTheme(theme === 'dark' ? 'light' : 'dark'),
    });

    if (densityLevel > 0) list.push({ id: 'density:down', label: 'Density: Denser', group: 'Actions', keywords: ['compact', 'dense', 'density', 'view', 'layout'], handler: () => setDensity(densityLevel - 1) });
    if (densityLevel < 8) list.push({ id: 'density:up',   label: 'Density: Spacious', group: 'Actions', keywords: ['spacious', 'cozy', 'comfy', 'airy', 'density', 'view', 'layout'], handler: () => setDensity(densityLevel + 1) });

    // ── Navigate ─────────────────────────────────────────────────────────────
    list.push({ id: 'nav:mail',    label: 'Go to Mail',        group: 'Navigate', keywords: ['mail', 'inbox', 'navigate', 'go', 'view'], handler: () => { setView('mail'); } });
    list.push({ id: 'nav:calendar', label: 'Go to Calendar',   group: 'Navigate', keywords: ['calendar', 'events', 'schedule', 'navigate', 'go', 'view'], handler: () => { setView('calendar'); } });
    list.push({ id: 'nav:all',     label: 'All Inboxes',       group: 'Navigate', keywords: ['inbox', 'all', 'navigate', 'go'],     handler: () => setSelectedFolder('smart:all') });
    list.push({ id: 'nav:unread',  label: 'Unread',            group: 'Navigate', keywords: ['unread', 'navigate', 'go'],           handler: () => setSelectedFolder('smart:unread') });
    list.push({ id: 'nav:flagged', label: 'Flagged',           group: 'Navigate', keywords: ['flagged', 'starred', 'navigate', 'go'], handler: () => setSelectedFolder('smart:flagged') });
    list.push({ id: 'nav:snoozed', label: 'Snoozed',           group: 'Navigate', keywords: ['snoozed', 'snooze', 'navigate', 'go', 'later'], handler: () => setSelectedFolder('smart:snoozed') });

    for (const lbl of labels) {
      list.push({
        id: `label:${lbl.id}`,
        label: lbl.name,
        hint: 'label',
        group: 'Labels',
        keywords: ['label', 'tag', 'navigate', 'go', lbl.name.toLowerCase()],
        handler: () => setSelectedLabel(lbl.id),
      });
    }

    const multiAccount = accounts.length > 1;
    for (const folder of folders.filter((f) => !f.isExcluded)) {
      const account = accounts.find((a) => a.id === folder.accountId);
      list.push({
        id: `folder:${folder.id}`,
        label: folder.name,
        hint: multiAccount ? account?.email : folder.fullPath !== folder.name ? folder.fullPath : undefined,
        group: 'Folders',
        keywords: ['go', 'folder', 'navigate', folder.name.toLowerCase(), folder.fullPath.toLowerCase()],
        handler: () => setSelectedFolder(folder.id),
      });
    }

    if (selectedMessage) {
      list.push({
        id: 'msg:reply',
        label: 'Reply',
        hint: 'r',
        group: 'Message',
        keywords: ['reply', 'respond'],
        handler: () => openCompose({
          accountId: selectedMessage.accountId,
          to: selectedMessage.fromEmail ?? '',
          subject: `Re: ${selectedMessage.subject ?? ''}`,
          inReplyTo: selectedMessage.id,
          mode: 'reply',
        }),
      });
      list.push({
        id: 'msg:forward',
        label: 'Forward',
        hint: 'f',
        group: 'Message',
        keywords: ['forward'],
        handler: () => openCompose({
          accountId: selectedMessage.accountId,
          to: '',
          subject: `Fwd: ${selectedMessage.subject ?? ''}`,
          inReplyTo: selectedMessage.id,
          mode: 'forward',
        }),
      });
      list.push({
        id: 'msg:archive',
        label: 'Archive Message',
        hint: 'e',
        group: 'Message',
        keywords: ['archive', 'message'],
        handler: () => api.post(`/api/v1/messages/${selectedMessage.id}/archive`)
          .then(() => removeMessage(selectedMessage.id)),
      });
      list.push({
        id: 'msg:delete',
        label: 'Delete Message',
        hint: '#',
        group: 'Message',
        keywords: ['delete', 'trash', 'remove'],
        handler: () => api.delete(`/api/v1/messages/${selectedMessage.id}`)
          .then(() => removeMessage(selectedMessage.id)),
      });
      list.push({
        id: selectedMessage.isRead ? 'msg:mark-unread' : 'msg:mark-read',
        label: selectedMessage.isRead ? 'Mark as Unread' : 'Mark as Read',
        hint: 'u',
        group: 'Message',
        keywords: ['read', 'unread', 'mark'],
        handler: () => api.patch(`/api/v1/messages/${selectedMessage.id}`, { isRead: !selectedMessage.isRead }).then(() => patchMessage(selectedMessage.id, { isRead: !selectedMessage.isRead })),
      });
      list.push({
        id: selectedMessage.isFlagged ? 'msg:unflag' : 'msg:flag',
        label: selectedMessage.isFlagged ? 'Remove Flag' : 'Flag Message',
        group: 'Message',
        keywords: ['flag', 'star', 'unflag'],
        handler: () => api.patch(`/api/v1/messages/${selectedMessage.id}`, { isFlagged: !selectedMessage.isFlagged }).then(() => patchMessage(selectedMessage.id, { isFlagged: !selectedMessage.isFlagged })),
      });
    }

    return list;
  }, [folders, accounts, labels, selectedMessage, theme, densityLevel, view, openCompose, setSelectedFolder, setSelectedLabel, setView, openSettings, setTheme, setDensity, openAdvancedSearch, patchMessage, removeMessage]);

  const filtered = useMemo(() => {
    if (!query.trim()) return actions.slice(0, 14);
    return actions
      .map((a) => ({ action: a, score: matchScore(a, query) }))
      .filter((x) => x.score > 0)
      .sort((a, b) => b.score - a.score)
      .slice(0, 14)
      .map((x) => x.action);
  }, [actions, query]);

  useEffect(() => setActiveIdx(0), [filtered]);

  useEffect(() => {
    const el = listRef.current?.querySelector(`[data-idx="${activeIdx}"]`) as HTMLElement | null;
    el?.scrollIntoView({ block: 'nearest' });
  }, [activeIdx]);

  useEffect(() => {
    if (!paletteOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setActiveIdx((i) => Math.min(i + 1, filtered.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setActiveIdx((i) => Math.max(i - 1, 0));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const action = filtered[activeIdx];
        if (action) { action.handler(); closePalette(); }
      } else if (e.key === 'Escape') {
        closePalette();
      }
    };
    window.addEventListener('keydown', handler, { capture: true });
    return () => window.removeEventListener('keydown', handler, { capture: true });
  }, [paletteOpen, filtered, activeIdx, closePalette]);

  if (!paletteOpen) return null;

  return (
    <div className={styles.overlay} onMouseDown={closePalette}>
      <div className={styles.palette} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.inputRow}>
          <svg className={styles.searchIcon} viewBox="0 0 16 16" fill="none">
            <circle cx="6.5" cy="6.5" r="4.5" stroke="currentColor" strokeWidth="1.5" />
            <path d="M10.5 10.5L14 14" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          </svg>
          <input
            ref={inputRef}
            className={styles.input}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Type a command or search…"
            autoComplete="off"
            spellCheck={false}
          />
          {query && (
            <button className={styles.clear} onMouseDown={(e) => { e.preventDefault(); setQuery(''); inputRef.current?.focus(); }}>
              ×
            </button>
          )}
        </div>
        {filtered.length > 0 ? (
          <div className={styles.results} ref={listRef}>
            {filtered.map((action, i) => {
              const isFirstItem = i === 0;
              const showHeader = !query.trim() && (isFirstItem || filtered[i - 1].group !== action.group);
              return (
                <div key={action.id}>
                  {showHeader && action.group && (
                    <div className={`${styles.groupHeader} ${isFirstItem ? '' : styles.groupHeaderBorder}`}>{action.group}</div>
                  )}
                  <div
                    data-idx={i}
                    className={`${styles.item} ${i === activeIdx ? styles.active : ''}`}
                    onMouseEnter={() => setActiveIdx(i)}
                    onMouseDown={(e) => { e.preventDefault(); action.handler(); closePalette(); }}
                  >
                    <span className={styles.label}>{action.label}</span>
                    {action.hint && <span className={styles.hint}>{action.hint}</span>}
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          <div className={styles.empty}>No results for &ldquo;{query}&rdquo;</div>
        )}
      </div>
    </div>
  );
}
