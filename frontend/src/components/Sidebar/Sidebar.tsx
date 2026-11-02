import React, { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { api, reportApiError } from '../../api/client';
import styles from './Sidebar.module.css';
import { useAppStore } from '../../store';
import { useApi, useSyncEvents } from '../../hooks/useApi';
import { AddAccountModal } from '../AccountSetup/AddAccountModal';
import { AdvancedSearchModal } from '../AdvancedSearch/AdvancedSearchModal';
import { useContextMenu } from '../ContextMenu/ContextMenu';
import type { Account, CalendarSuggestion, Folder, Label, Suggestion, SuggestResponse } from '../../types';

type SuggestItem =
  | { kind: 'message'; data: Suggestion }
  | { kind: 'event'; data: CalendarSuggestion };

// ── SVG icons ────────────────────────────────────────────────────────────────

const IC = { fill: 'none', stroke: 'currentColor', strokeWidth: 1.5, strokeLinecap: 'round' as const, strokeLinejoin: 'round' as const };

function InboxIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" {...IC}>
      <rect x="1.5" y="3" width="13" height="10" rx="1.5" />
      <path d="M1.5 9.5h3.8l1.7 2.5h3l1.7-2.5h3.8" />
    </svg>
  );
}

function SentIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" {...IC}>
      <path d="M13.5 2.5L2 7l5 2 1.5 4.5 5-11z" />
      <path d="M7 9l3-2.5" />
    </svg>
  );
}

function DraftIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" {...IC}>
      <path d="M10.5 2H4a1.5 1.5 0 00-1.5 1.5v9A1.5 1.5 0 004 14h8a1.5 1.5 0 001.5-1.5V6L10.5 2z" />
      <path d="M10.5 2v4H14" />
      <path d="M5 9.5h6M5 12h3.5" />
    </svg>
  );
}

function ArchiveIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" {...IC}>
      <rect x="1.5" y="2" width="13" height="3" rx="1" />
      <path d="M2.5 5v7.5a1 1 0 001 1h9a1 1 0 001-1V5" />
      <path d="M6 9h4" />
    </svg>
  );
}

function TrashIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" {...IC}>
      <path d="M2.5 4.5h11" />
      <path d="M5.5 4.5v-1a.5.5 0 01.5-.5h3a.5.5 0 01.5.5v1" />
      <path d="M3.5 4.5l.7 8.5a1 1 0 001 .9h5.6a1 1 0 001-.9l.7-8.5" />
    </svg>
  );
}

function SpamIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" {...IC}>
      <path d="M5.5 1.5h5l4 4v5l-4 4h-5l-4-4v-5l4-4z" />
      <path d="M8 5.5v3M8 10v.5" />
    </svg>
  );
}

function FolderIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" {...IC}>
      <path d="M1.5 4.5a1 1 0 011-1h3.6l1.8 2H13a1 1 0 011 1v5.5a1 1 0 01-1 1H2.5a1 1 0 01-1-1V4.5z" />
    </svg>
  );
}

function ChevronIcon({ collapsed }: { collapsed: boolean }) {
  return (
    <svg
      viewBox="0 0 10 10"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      width="10"
      height="10"
      style={{ transform: collapsed ? 'rotate(-90deg)' : 'rotate(0deg)', transition: 'transform 0.15s', flexShrink: 0 }}
    >
      <path d="M2 3.5l3 3 3-3" />
    </svg>
  );
}

function SearchIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" {...IC}>
      <circle cx="7" cy="7" r="4.5" />
      <path d="M10.5 10.5L14 14" />
    </svg>
  );
}

function TuneIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" {...IC}>
      <path d="M1.5 4h13M4 8h8M6.5 12h3" />
    </svg>
  );
}

function ComposeIcon() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" width="13" height="13">
      <path d="M11 2.5l2.5 2.5-7.5 7.5H3.5v-2.5L11 2.5z" />
    </svg>
  );
}

function GearIcon({ theme }: { theme: string }) {
  return (
    <img
      src={theme === 'dark' ? '/icons/settings-dark.png' : '/icons/settings-light.png'}
      alt=""
      width="14"
      height="14"
      style={{ display: 'block' }}
    />
  );
}

function SunIcon() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" width="14" height="14">
      <circle cx="8" cy="8" r="3" />
      <path d="M8 1.5v1.5M8 13v1.5M1.5 8H3M13 8h1.5M3.5 3.5l1 1M11.5 11.5l1 1M3.5 12.5l1-1M11.5 4.5l1-1" />
    </svg>
  );
}

function MoonIcon() {
  return (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" width="14" height="14">
      <path d="M12.5 10.5A5.5 5.5 0 016 3a5.5 5.5 0 100 11 5.5 5.5 0 006.5-3.5z" />
    </svg>
  );
}

// Smart folder icons
function AllInboxIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" {...IC}>
      <rect x="1.5" y="4.5" width="13" height="9" rx="1.5" />
      <path d="M1.5 10.5h3.5l1.5 2h3l1.5-2h3.5" />
      <path d="M4.5 4.5V3.5a1 1 0 011-1h5a1 1 0 011 1v1" />
    </svg>
  );
}

function UnreadIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" {...IC}>
      <circle cx="11.5" cy="4.5" r="2.5" fill="var(--accent)" stroke="none" />
      <rect x="1.5" y="3" width="13" height="10" rx="1.5" />
      <path d="M1.5 9.5h3.8l1.7 2.5h3l1.7-2.5h3.8" />
    </svg>
  );
}

function FlaggedIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" {...IC}>
      <path d="M3.5 2v12" />
      <path d="M3.5 2H12l-2.5 4L12 10H3.5" />
    </svg>
  );
}

function SnoozedIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" {...IC}>
      <circle cx="8" cy="9" r="5" />
      <path d="M8 6v3l2 1.5" />
      <path d="M5.5 2.5h5" />
      <path d="M6 3L8 1.5 10 3" />
    </svg>
  );
}

function CalendarNavIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 16 16" {...IC}>
      <rect x="1.5" y="3" width="13" height="11" rx="1.5" />
      <path d="M1.5 7.5h13" />
      <path d="M5 1.5v3M11 1.5v3" />
    </svg>
  );
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function folderDepth(folder: Folder): number {
  const normalized = folder.fullPath.replace(/^\[Gmail\]\//, '');
  return normalized.split('/').length - 1;
}

function getFolderIcon(specialUse: Folder['specialUse']) {
  switch (specialUse) {
    case 'inbox':   return InboxIcon;
    case 'sent':    return SentIcon;
    case 'drafts':  return DraftIcon;
    case 'archive': return ArchiveIcon;
    case 'trash':   return TrashIcon;
    case 'spam':    return SpamIcon;
    default:        return FolderIcon;
  }
}

function getAccountColor(account: Account): string {
  if (account.color) return account.color;
  const hue = account.email.split('').reduce((n, c) => n + c.charCodeAt(0), 0) % 360;
  return `oklch(58% 0.16 ${hue})`;
}

function getAvatarStyle(name: string): { bg: string; initials: string } {
  const hue = name.split('').reduce((n, c) => n + c.charCodeAt(0), 0) % 360;
  return {
    bg: `oklch(62% 0.12 ${hue})`,
    initials: name.split(' ').map((w) => w[0]).join('').slice(0, 2).toUpperCase(),
  };
}

// ── Smart folders ─────────────────────────────────────────────────────────────

interface SmartFolder {
  id: string;
  label: string;
  Icon: (props: { className?: string }) => React.ReactElement;
}

const SMART_FOLDERS: SmartFolder[] = [
  { id: 'smart:all',     label: 'All Inboxes', Icon: AllInboxIcon },
  { id: 'smart:unread',  label: 'Unread',       Icon: UnreadIcon  },
  { id: 'smart:flagged', label: 'Flagged',      Icon: FlaggedIcon },
  { id: 'smart:snoozed', label: 'Snoozed',      Icon: SnoozedIcon },
];

// ── Per-account folders hook ──────────────────────────────────────────────────

interface AccountFolders {
  account: Account;
  folders: Folder[] | undefined;
}

// We fetch folders for each account using stable hook slots (hooks can't be in loops).
// MAX_ACCOUNTS caps how many accounts we support in the sidebar.
const MAX_ACCOUNTS = 8;

function useAllAccountFolders(accounts: Account[] | undefined): {
  accountFolders: AccountFolders[];
  allFolderData: (Folder[] | undefined)[];
  refetchAll: () => void;
} {
  const a = accounts ?? [];

  const r0 = useApi<Folder[]>(a[0] ? `/api/v1/accounts/${a[0].id}/folders` : '', { immediate: !!a[0] });
  const r1 = useApi<Folder[]>(a[1] ? `/api/v1/accounts/${a[1].id}/folders` : '', { immediate: !!a[1] });
  const r2 = useApi<Folder[]>(a[2] ? `/api/v1/accounts/${a[2].id}/folders` : '', { immediate: !!a[2] });
  const r3 = useApi<Folder[]>(a[3] ? `/api/v1/accounts/${a[3].id}/folders` : '', { immediate: !!a[3] });
  const r4 = useApi<Folder[]>(a[4] ? `/api/v1/accounts/${a[4].id}/folders` : '', { immediate: !!a[4] });
  const r5 = useApi<Folder[]>(a[5] ? `/api/v1/accounts/${a[5].id}/folders` : '', { immediate: !!a[5] });
  const r6 = useApi<Folder[]>(a[6] ? `/api/v1/accounts/${a[6].id}/folders` : '', { immediate: !!a[6] });
  const r7 = useApi<Folder[]>(a[7] ? `/api/v1/accounts/${a[7].id}/folders` : '', { immediate: !!a[7] });

  const dataSlots = [r0.data, r1.data, r2.data, r3.data, r4.data, r5.data, r6.data, r7.data];
  const refetchSlots = [r0.refetch, r1.refetch, r2.refetch, r3.refetch, r4.refetch, r5.refetch, r6.refetch, r7.refetch];

  const accountFolders: AccountFolders[] = a.slice(0, MAX_ACCOUNTS).map((account, i) => ({
    account,
    folders: dataSlots[i],
  }));

  const refetchAll = useCallback(() => {
    refetchSlots.slice(0, a.length).forEach((fn) => fn());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [a.length, ...refetchSlots]);

  return { accountFolders, allFolderData: dataSlots, refetchAll };
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatSuggestDate(dateStr: string | null): string {
  if (!dateStr) return '';
  const d = new Date(dateStr);
  const now = new Date();
  const sameYear = d.getFullYear() === now.getFullYear();
  return d.toLocaleDateString(undefined, sameYear
    ? { month: 'short', day: 'numeric' }
    : { month: 'short', day: 'numeric', year: 'numeric' });
}

// ── Component ────────────────────────────────────────────────────────────────

interface SidebarProps {
  onAccountAdded: () => void;
}

export function Sidebar({ onAccountAdded }: SidebarProps) {
  const [showAddModal, setShowAddModal] = useState(false);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [suggestions, setSuggestions] = useState<SuggestItem[]>([]);
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [activeSuggestion, setActiveSuggestion] = useState(-1);
  const [dropdownPos, setDropdownPos] = useState({ top: 0, left: 0, width: 0 });
  const searchInputRef = useRef<HTMLInputElement>(null);

  const { selectedFolderId, selectedLabelId, setSelectedFolder, setSelectedLabel, setFolders, setLabels, openCompose, searchQuery, setSearchQuery, conditionGroup, navigateToMessage, advancedSearchOpen, openAdvancedSearch, closeAdvancedSearch, theme, densityLevel, setTheme, setDensity, openSettings, setView, view, setSelectedCalendarEvent } = useAppStore();
  const navRef = useRef<HTMLElement>(null);
  const { contextMenu, openContextMenu } = useContextMenu();

  const toggle = (key: string) => setCollapsed((c) => ({ ...c, [key]: !c[key] }));

  // Fetch autocomplete suggestions with 150ms debounce
  useEffect(() => {
    const q = searchQuery.trim();
    if (q.length < 2) {
      setSuggestions([]);
      setShowSuggestions(false);
      setActiveSuggestion(-1);
      return;
    }
    const t = setTimeout(async () => {
      try {
        {
          const data = await api.get<SuggestResponse>(`/api/v1/search/suggest?q=${encodeURIComponent(q)}`);
          const items: SuggestItem[] = [
            ...data.messages.map((m): SuggestItem => ({ kind: 'message', data: m })),
            ...data.events.map((e): SuggestItem => ({ kind: 'event', data: e })),
          ];
          setSuggestions(items);
          setShowSuggestions(items.length > 0);
          setActiveSuggestion(-1);
        }
      } catch (e) { reportApiError(e); }
    }, 150);
    return () => clearTimeout(t);
  }, [searchQuery]);

  // Recalculate dropdown position whenever it opens
  useLayoutEffect(() => {
    if (showSuggestions && searchInputRef.current) {
      const r = searchInputRef.current.getBoundingClientRect();
      setDropdownPos({ top: r.bottom + 4, left: r.left, width: Math.max(r.width, 340) });
    }
  }, [showSuggestions, suggestions.length]);

  const closeSuggestions = () => {
    setShowSuggestions(false);
    setActiveSuggestion(-1);
  };

  const handleSuggestionClick = (s: SuggestItem) => {
    closeSuggestions();
    setSuggestions([]);
    if (s.kind === 'message') {
      navigateToMessage(s.data.folderId, s.data.id);
    } else {
      setView('calendar');
      setSelectedCalendarEvent(s.data.id);
    }
  };

  const handleSearchKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (!showSuggestions || suggestions.length === 0) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setActiveSuggestion((i) => Math.min(i + 1, suggestions.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActiveSuggestion((i) => Math.max(i - 1, -1));
    } else if (e.key === 'Enter' && activeSuggestion >= 0) {
      e.preventDefault();
      handleSuggestionClick(suggestions[activeSuggestion]);
    } else if (e.key === 'Escape') {
      closeSuggestions();
    }
  };

  const { data: accounts } = useApi<Account[]>('/api/v1/accounts');
  const { accountFolders, allFolderData, refetchAll } = useAllAccountFolders(accounts);

  // Fetch labels for the first account (labels are per-account; use first account for the sidebar).
  const firstAccountId = accounts?.[0]?.id ?? '';
  const { data: labels } = useApi<Label[]>(
    firstAccountId ? `/api/v1/labels?account_id=${firstAccountId}` : '',
    { immediate: !!firstAccountId },
  );

  const handleSync = useCallback(() => {
    refetchAll();
  }, [refetchAll]);

  const syncProgress = useSyncEvents(handleSync);

  // Merge all fetched folders into the store so MessageList can look up names.
  // Depend on the individual data-slot references so this only fires when a
  // fetch resolves — not on every render.
  useEffect(() => {
    const all: Folder[] = allFolderData.flatMap((d) => d ?? []);
    if (all.length > 0) {
      setFolders(all);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...allFolderData, setFolders]);

  // Sync labels into the store so MessageList can display the label name in the header.
  useEffect(() => {
    if (labels) {
      setLabels(labels);
    }
  }, [labels, setLabels]);

  const specialOrder: Folder['specialUse'][] = ['inbox', 'sent', 'drafts', 'archive', 'trash', 'spam'];

  return (
    <aside className={styles.sidebar}>
      <button
        className={styles.compose}
        type="button"
        onClick={() => {
          const accountId = accounts?.[0]?.id;
          if (accountId) openCompose({ accountId, to: '', subject: '', mode: 'compose' });
        }}
      >
        <ComposeIcon />
        Compose
      </button>

      {syncProgress && (
        <div className={styles.syncStrip}>
          <div className={styles.syncRow}>
            {syncProgress.syncing ? (
              <span className={styles.syncSpinner} />
            ) : (
              <svg className={styles.syncDone} viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.5">
                <path d="M2 5l2.5 2.5L8 3" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            )}
            <span className={styles.syncText}>
              {syncProgress.syncing
                ? syncProgress.total > 0
                  ? `Syncing ${syncProgress.done}/${syncProgress.total} folders…`
                  : 'Syncing…'
                : 'Sync complete'}
            </span>
          </div>
          {syncProgress.syncing && syncProgress.total > 0 && (
            <div className={styles.syncBar}>
              <div
                className={styles.syncBarFill}
                style={{ width: `${Math.round((syncProgress.done / syncProgress.total) * 100)}%` }}
              />
            </div>
          )}
        </div>
      )}

      <div className={styles.searchWrap}>
        <SearchIcon className={styles.searchIcon} />
        <input
          ref={searchInputRef}
          className={styles.searchInput}
          type="text"
          placeholder="Search mail & calendar…"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          onKeyDown={handleSearchKeyDown}
          onFocus={() => { if (suggestions.length > 0) setShowSuggestions(true); }}
          onBlur={() => setTimeout(closeSuggestions, 150)}
          autoComplete="off"
        />
        <button
          className={`${styles.filterBtn}${conditionGroup ? ` ${styles.filterBtnActive}` : ''}`}
          type="button"
          title="Advanced search"
          onClick={() => openAdvancedSearch()}
        >
          <TuneIcon />
        </button>
      </div>

      {showSuggestions && suggestions.length > 0 && (
        <div
          className={styles.suggestDropdown}
          style={{ position: 'fixed', top: dropdownPos.top, left: dropdownPos.left, width: dropdownPos.width }}
          onMouseDown={(e) => e.preventDefault()}
        >
          {suggestions.map((s, i) => (
            <div
              key={`${s.kind}-${s.kind === 'message' ? s.data.id : s.data.id}`}
              className={`${styles.suggestRow}${i === activeSuggestion ? ` ${styles.suggestRowActive}` : ''}`}
              onClick={() => handleSuggestionClick(s)}
            >
              {s.kind === 'message' ? (
                <>
                  <span className={styles.suggestFrom}>{s.data.fromName || s.data.fromEmail || 'Unknown'}</span>
                  <span className={styles.suggestDate}>{formatSuggestDate(s.data.date)}</span>
                  <span className={styles.suggestSubject}>{s.data.subject || '(no subject)'}</span>
                </>
              ) : (
                <>
                  <span className={`${styles.suggestFrom} ${styles.suggestEventLabel}`}>
                    <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" width="11" height="11" style={{ flexShrink: 0 }}>
                      <rect x="1" y="2" width="10" height="9" rx="1" />
                      <path d="M4 1v2M8 1v2M1 5h10" />
                    </svg>
                    {s.data.title}
                  </span>
                  <span className={styles.suggestDate}>{formatSuggestDate(s.data.startAt)}</span>
                </>
              )}
            </div>
          ))}
          <div className={styles.suggestFooter}>↵ Enter to search all results</div>
        </div>
      )}

      {advancedSearchOpen && <AdvancedSearchModal onClose={closeAdvancedSearch} />}

      {/* Smart / virtual folders — pinned, never scrolls */}
      <div className={styles.smartSection}>
        {SMART_FOLDERS.map(({ id, label, Icon }) => {
          const isActive = view === 'mail' && id === selectedFolderId;
          return (
            <div
              key={id}
              className={`${styles.folderItem}${isActive ? ` ${styles.active}` : ''}`}
              role="button"
              tabIndex={0}
              onClick={() => { setSelectedFolder(id); if (navRef.current) navRef.current.scrollTop = 0; }}
              onKeyDown={(e) => { if (e.key === 'Enter') { setSelectedFolder(id); if (navRef.current) navRef.current.scrollTop = 0; } }}
              onContextMenu={(e) => openContextMenu(e, [
                { label: 'Open folder', action: () => { setSelectedFolder(id); if (navRef.current) navRef.current.scrollTop = 0; } },
              ])}
            >
              <Icon className={styles.folderIcon} />
              <span className={styles.folderName}>{label}</span>
            </div>
          );
        })}
        <div
          className={`${styles.folderItem}${view === 'calendar' ? ` ${styles.active}` : ''}`}
          role="button"
          tabIndex={0}
          onClick={() => setView('calendar')}
          onKeyDown={(e) => { if (e.key === 'Enter') setView('calendar'); }}
        >
          <CalendarNavIcon className={styles.folderIcon} />
          <span className={styles.folderName}>Calendar</span>
        </div>
      </div>

      {/* Labels section */}
      {labels && labels.length > 0 && (
        <div className={styles.labelsSection}>
          {labels.map((label) => {
            const isActive = view === 'mail' && selectedLabelId === label.id;
            return (
              <div
                key={label.id}
                className={`${styles.folderItem}${isActive ? ` ${styles.active}` : ''}`}
                role="button"
                tabIndex={0}
                onClick={() => setSelectedLabel(label.id)}
                onKeyDown={(e) => { if (e.key === 'Enter') setSelectedLabel(label.id); }}
              >
                <span className={styles.labelDot} style={{ background: label.color }} />
                <span className={styles.folderName}>{label.name}</span>
              </div>
            );
          })}
        </div>
      )}

      {/* Divider between smart folders/labels and IMAP folders */}
      {accountFolders.length > 0 && <div className={styles.divider} />}

      {/* Scrollable per-account IMAP folders */}
      <nav className={styles.nav} ref={navRef}>
        {accountFolders.map(({ account, folders }) => {
          if (!folders) return null;

          const accountColor = getAccountColor(account);
          const accountKey = `account:${account.id}`;
          const foldersKey = `folders:${account.id}`;
          const accountCollapsed = !!collapsed[accountKey];
          const foldersCollapsed = !!collapsed[foldersKey];

          const special = specialOrder
            .map((use) => folders.find((f) => f.specialUse === use))
            .filter((f): f is Folder => !!f);
          const other = folders.filter((f) => !f.specialUse);
          const accountUnread = folders.reduce((n, f) => n + (f.unreadCount ?? 0), 0);

          return (
            <div key={account.id} className={styles.accountSection}>
              <button
                className={styles.accountSectionHeader}
                type="button"
                onClick={() => toggle(accountKey)}
              >
                <ChevronIcon collapsed={accountCollapsed} />
                <span className={styles.accountSectionName}>{account.name}</span>
                {accountCollapsed && accountUnread > 0 && (
                  <span className={styles.badge}>{accountUnread}</span>
                )}
              </button>

              {!accountCollapsed && (
                <>
                  {special.map((folder) => {
                    const Icon = getFolderIcon(folder.specialUse);
                    const isActive = folder.id === selectedFolderId;
                    return (
                      <div
                        key={folder.id}
                        className={`${styles.folderItem}${isActive ? ` ${styles.active}` : ''}`}
                        role="button"
                        tabIndex={0}
                        onClick={() => setSelectedFolder(folder.id)}
                        onKeyDown={(e) => e.key === 'Enter' && setSelectedFolder(folder.id)}
                        onContextMenu={(e) => openContextMenu(e, [
                          { label: 'Open folder', action: () => setSelectedFolder(folder.id) },
                          { separator: true },
                          {
                            label: 'Mark all as read',
                            action: () => {
                              api.post(`/api/v1/folders/${folder.id}/mark-read`).catch(() => undefined);
                            },
                          },
                        ])}
                      >
                        <span className={styles.accountColorDot} style={{ background: accountColor }} />
                        <Icon className={styles.folderIcon} />
                        <span className={styles.folderName}>{folder.name}</span>
                        {folder.unreadCount > 0 && (
                          <span className={styles.badge}>{folder.unreadCount}</span>
                        )}
                      </div>
                    );
                  })}

                  {other.length > 0 && (
                    <>
                      <button
                        className={styles.folderGroupHeader}
                        type="button"
                        onClick={() => toggle(foldersKey)}
                      >
                        <ChevronIcon collapsed={foldersCollapsed} />
                        <span>Folders</span>
                        <span className={styles.folderGroupCount}>{other.length}</span>
                      </button>
                      {!foldersCollapsed && other.map((folder) => {
                        const isActive = folder.id === selectedFolderId;
                        const depth = folderDepth(folder);
                        return (
                          <div
                            key={folder.id}
                            className={`${styles.folderItem}${isActive ? ` ${styles.active}` : ''}`}
                            style={{ paddingLeft: `${18 + depth * 12}px` }}
                            role="button"
                            tabIndex={0}
                            onClick={() => setSelectedFolder(folder.id)}
                            onKeyDown={(e) => e.key === 'Enter' && setSelectedFolder(folder.id)}
                            onContextMenu={(e) => openContextMenu(e, [
                              { label: 'Open folder', action: () => setSelectedFolder(folder.id) },
                              { separator: true },
                              {
                                label: 'Mark all as read',
                                action: () => {
                                  api.post(`/api/v1/folders/${folder.id}/mark-read`).catch(() => undefined);
                                },
                              },
                            ])}
                          >
                            <span className={styles.accountColorDot} style={{ background: accountColor }} />
                            <FolderIcon className={styles.folderIcon} />
                            <span className={styles.folderName}>{folder.name}</span>
                            {folder.unreadCount > 0 && (
                              <span className={styles.badge}>{folder.unreadCount}</span>
                            )}
                          </div>
                        );
                      })}
                    </>
                  )}
                </>
              )}
            </div>
          );
        })}
      </nav>

      <div className={styles.accounts}>
        {accounts?.map((account) => {
          const avatar = getAvatarStyle(account.name);
          return (
            <div key={account.id} className={styles.accountItem}>
              <div className={styles.accountAvatar} style={{ background: avatar.bg }}>
                {avatar.initials}
              </div>
              <span className={styles.accountEmail}>{account.email}</span>
            </div>
          );
        })}
        <button className={styles.addAccountBtn} type="button" onClick={() => setShowAddModal(true)}>
          + Add account
        </button>
      </div>

      <div className={styles.footer}>
        <div className={styles.densityGroup} role="group" aria-label="Message density">
          <button
            type="button"
            title="Denser"
            className={styles.densityBtn}
            onClick={() => setDensity(densityLevel - 1)}
            disabled={densityLevel <= 0}
            aria-label="Denser"
          >−</button>
          <button
            type="button"
            title="More spacious"
            className={styles.densityBtn}
            onClick={() => setDensity(densityLevel + 1)}
            disabled={densityLevel >= 8}
            aria-label="More spacious"
          >+</button>
        </div>
        <div className={styles.footerRight}>
          <button
            type="button"
            className={styles.footerBtn}
            title={theme === 'light' ? 'Switch to dark mode' : 'Switch to light mode'}
            onClick={() => setTheme(theme === 'light' ? 'dark' : 'light')}
          >
            {theme === 'light' ? <MoonIcon /> : <SunIcon />}
          </button>
          <button
            type="button"
            className={styles.footerBtn}
            title="Settings"
            onClick={openSettings}
          >
            <GearIcon theme={theme} />
          </button>
        </div>
      </div>

      {showAddModal && (
        <AddAccountModal
          onClose={() => setShowAddModal(false)}
          onAccountAdded={() => {
            setShowAddModal(false);
            onAccountAdded();
          }}
        />
      )}

      {contextMenu}
    </aside>
  );
}
