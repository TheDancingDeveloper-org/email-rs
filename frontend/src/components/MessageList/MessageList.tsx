import { useCallback, useEffect, useRef, useState } from 'react';
import styles from './MessageList.module.css';
import { MessageRow } from './MessageRow';
import { useAppStore } from '../../store';
import { useApi, useSyncEvents } from '../../hooks/useApi';
import { useContextMenu } from '../ContextMenu/ContextMenu';
import type { ContextMenuItem } from '../ContextMenu/ContextMenu';
import type { Message } from '../../types';
import { conditionGroupToSearchUrl } from '../../utils/search';

// ── Thread grouping ───────────────────────────────────────────────────────────

interface ThreadGroup {
  key: string;
  latest: Message;
  messages: Message[];
}

function groupByThread(messages: Message[]): ThreadGroup[] {
  const map = new Map<string, Message[]>();
  for (const msg of messages) {
    const key = msg.threadId ?? msg.id;
    const group = map.get(key);
    if (group) group.push(msg);
    else map.set(key, [msg]);
  }
  return Array.from(map.values()).map((msgs) => {
    const sorted = [...msgs].sort((a, b) => {
      const da = a.date ? new Date(a.date).getTime() : 0;
      const db = b.date ? new Date(b.date).getTime() : 0;
      return db - da;
    });
    return { key: msgs[0].threadId ?? msgs[0].id, latest: sorted[0], messages: sorted };
  });
}

// ── Keyboard navigation ───────────────────────────────────────────────────────

function useKeyboardNav(
  messages: Message[] | undefined,
  selectedId: string | null,
  setSelected: (id: string | null) => void,
) {
  useEffect(() => {
    if (!messages) return;
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA') return;
      if (e.key !== 'j' && e.key !== 'k') return;
      const idx = selectedId ? messages.findIndex((m) => m.id === selectedId) : -1;
      if (e.key === 'j') {
        const next = messages[idx + 1];
        if (next) setSelected(next.id);
      } else {
        const prev = messages[Math.max(0, idx - 1)];
        if (prev) setSelected(prev.id);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [messages, selectedId, setSelected]);
}

// ── Bulk API helper ───────────────────────────────────────────────────────────

type BulkAction = 'archive' | 'delete' | 'mark_read' | 'mark_unread';

interface BulkResponse { processed: number; failed: number; results: { id: string; success: boolean; error?: string }[] }

async function callBulkApi(ids: string[], action: BulkAction): Promise<BulkResponse> {
  const response = await fetch('/api/v1/messages/bulk', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ids, action }),
  });
  const body = await response.json() as BulkResponse & { error?: string };
  if (!response.ok) throw new Error(body.error || `${response.status} ${response.statusText}`);
  return body;
}

// ── Component ─────────────────────────────────────────────────────────────────

export function MessageList() {
  const pageSize = 50;
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(false);
  const loadedMessagesRef = useRef<Message[]>([]);
  const [bulkError, setBulkError] = useState<string | null>(null);
  const {
    selectedFolderId,
    selectedMessageId,
    setSelectedMessage,
    folders,
    labels,
    searchQuery,
    conditionGroup,
    folderSelectSeq,
    messages,
    setMessages,
    selectedMessageIds,
    toggleMessageSelection,
    selectAllMessages,
    clearMessageSelection,
  } = useAppStore();
  const listRef = useRef<HTMLDivElement>(null);
  const [expandedThreads, setExpandedThreads] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (listRef.current) listRef.current.scrollTop = 0;
    setExpandedThreads(new Set());
    setPage(1);
    setHasMore(false);
    loadedMessagesRef.current = [];
  }, [folderSelectSeq]);

  const isSmartFolder = !!selectedFolderId?.startsWith('smart:');
  const smartKind = isSmartFolder ? selectedFolderId!.replace('smart:', '') : '';
  const isLabelFolder = !!selectedFolderId?.startsWith('label:');
  const labelFolderId = isLabelFolder ? selectedFolderId!.replace('label:', '') : '';
  const isSearching = !!searchQuery.trim();
  const advancedSearchUrl = conditionGroup ? conditionGroupToSearchUrl(conditionGroup) : '';
  const isAdvancedSearching = !!advancedSearchUrl;

  const { data: folderMessages, loading: folderLoading, refetch } = useApi<Message[]>(
    selectedFolderId && !searchQuery && !isSmartFolder && !isLabelFolder && !isAdvancedSearching
      ? `/api/v1/folders/${selectedFolderId}/messages?page=${page}&per_page=${pageSize}`
      : '',
    { immediate: !!selectedFolderId && !searchQuery && !isSmartFolder && !isLabelFolder && !isAdvancedSearching },
  );

  const { data: labelMessages, loading: labelLoading, refetch: refetchLabel } = useApi<Message[]>(
    isLabelFolder && !searchQuery && !isAdvancedSearching ? `/api/v1/labels/${labelFolderId}/messages?page=${page}&per_page=${pageSize}` : '',
    { immediate: isLabelFolder && !searchQuery && !isAdvancedSearching },
  );

  const { data: smartMessages, loading: smartLoading, refetch: refetchSmart } = useApi<Message[]>(
    isSmartFolder && !searchQuery && !isAdvancedSearching ? `/api/v1/smart-folders/${smartKind}/messages?page=${page}&per_page=${pageSize}` : '',
    { immediate: isSmartFolder && !searchQuery && !isAdvancedSearching },
  );

  const [debouncedQuery, setDebouncedQuery] = useState(searchQuery);
  useEffect(() => {
    const t = setTimeout(() => setDebouncedQuery(searchQuery), 300);
    return () => clearTimeout(t);
  }, [searchQuery]);

  const { data: searchResultsRaw, loading: searchLoading } = useApi<{ messages: Message[] }>(
    debouncedQuery.trim() ? `/api/v1/search?q=${encodeURIComponent(debouncedQuery.trim())}` : '',
    { immediate: !!debouncedQuery.trim() },
  );
  const searchResults = searchResultsRaw?.messages;

  const { data: advancedResults, loading: advancedLoading } = useApi<Message[]>(
    advancedSearchUrl,
    { immediate: isAdvancedSearching },
  );

  const handleSync = useCallback(() => { refetch(); refetchSmart(); refetchLabel(); }, [refetch, refetchSmart, refetchLabel]);
  useSyncEvents(handleSync);

  const fetchedMessages = isAdvancedSearching ? advancedResults
    : isSearching ? searchResults
    : isSmartFolder ? smartMessages
    : isLabelFolder ? labelMessages
    : folderMessages;
  const loading = isAdvancedSearching ? advancedLoading
    : isSearching ? searchLoading
    : isSmartFolder ? smartLoading
    : isLabelFolder ? labelLoading
    : folderLoading;

  // Sync fetched data into the store so mutations elsewhere can update it.
  useEffect(() => {
    if (!fetchedMessages) return;
    const paginated = !isSearching && !isAdvancedSearching;
    if (paginated && page > 1) {
      const byId = new Map(loadedMessagesRef.current.map((message) => [message.id, message]));
      for (const message of fetchedMessages) byId.set(message.id, message);
      loadedMessagesRef.current = Array.from(byId.values());
    } else {
      loadedMessagesRef.current = fetchedMessages;
    }
    setMessages(loadedMessagesRef.current);
    setHasMore(paginated && fetchedMessages.length === pageSize);
  }, [fetchedMessages, isAdvancedSearching, isSearching, page, pageSize, setMessages]);

  useKeyboardNav(messages.length ? messages : undefined, selectedMessageId, setSelectedMessage);

  // ── Extra keyboard shortcuts ─────────────────────────────────────────────

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA') return;

      // Ctrl+A / Cmd+A — select all
      if ((e.ctrlKey || e.metaKey) && e.key === 'a') {
        e.preventDefault();
        selectAllMessages(messages.map((m) => m.id));
        return;
      }

      // Escape — clear selection or deselect message
      if (e.key === 'Escape') {
        if (selectedMessageIds.length > 0) {
          clearMessageSelection();
        } else {
          setSelectedMessage(null);
        }
        return;
      }

      if (!selectedMessageId) return;

      // x — toggle selection on focused row
      if (e.key === 'x') {
        toggleMessageSelection(selectedMessageId);
        return;
      }

      // e — archive focused message
      if (e.key === 'e') {
        fetch(`/api/v1/messages/${selectedMessageId}/archive`, { method: 'POST' }).then(() => {
          setSelectedMessage(null);
          refetch();
        }).catch(() => undefined);
        return;
      }

      // Delete / Backspace — trash focused message
      if (e.key === 'Delete' || e.key === 'Backspace') {
        fetch(`/api/v1/messages/${selectedMessageId}`, { method: 'DELETE' }).then(() => {
          setSelectedMessage(null);
          refetch();
        }).catch(() => undefined);
        return;
      }

      // u — toggle read
      if (e.key === 'u') {
        const msg = messages.find((m) => m.id === selectedMessageId);
        if (msg) {
          fetch(`/api/v1/messages/${selectedMessageId}`, {
            method: 'PATCH',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ isRead: !msg.isRead }),
          }).then(() => refetch()).catch(() => undefined);
        }
        return;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [
    messages,
    selectedMessageId,
    selectedMessageIds,
    toggleMessageSelection,
    selectAllMessages,
    clearMessageSelection,
    setSelectedMessage,
    refetch,
  ]);

  // ── Bulk action handlers ─────────────────────────────────────────────────

  const handleBulkAction = useCallback(async (action: BulkAction) => {
    if (action === 'delete') {
      if (!window.confirm(`Delete ${selectedMessageIds.length} message(s)?`)) return;
    }
    setBulkError(null);
    try {
      const result = await callBulkApi(selectedMessageIds, action);
      if (result.failed) {
        setBulkError(`${result.processed} succeeded, ${result.failed} failed: ${result.results.filter(item => !item.success).map(item => item.error).join('; ')}`);
      } else {
        clearMessageSelection();
      }
      refetch(); refetchSmart(); refetchLabel();
    } catch (error) {
      setBulkError(error instanceof Error ? error.message : String(error));
    }
  }, [selectedMessageIds, clearMessageSelection, refetch, refetchSmart, refetchLabel]);

  // ── Context menu ─────────────────────────────────────────────────────────

  const { contextMenu, openContextMenu } = useContextMenu();

  const buildRowContextMenu = useCallback((msg: Message): ContextMenuItem[] => [
    {
      label: 'Open',
      action: () => setSelectedMessage(msg.id),
    },
    {
      label: 'Reply',
      action: () => {
        setSelectedMessage(msg.id);
      },
    },
    {
      label: 'Forward',
      action: () => {
        setSelectedMessage(msg.id);
      },
    },
    { separator: true },
    {
      label: 'Archive',
      action: () => {
        fetch(`/api/v1/messages/${msg.id}/archive`, { method: 'POST' })
          .then(() => { refetch(); refetchSmart(); })
          .catch(() => undefined);
      },
    },
    {
      label: 'Move to Trash',
      action: () => {
        fetch(`/api/v1/messages/${msg.id}`, { method: 'DELETE' })
          .then(() => { if (selectedMessageId === msg.id) setSelectedMessage(null); refetch(); refetchSmart(); })
          .catch(() => undefined);
      },
    },
    { separator: true },
    {
      label: 'Mark Read',
      action: () => {
        fetch(`/api/v1/messages/${msg.id}`, {
          method: 'PATCH',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ isRead: true }),
        }).then(() => { refetch(); refetchSmart(); }).catch(() => undefined);
      },
    },
    {
      label: 'Mark Unread',
      action: () => {
        fetch(`/api/v1/messages/${msg.id}`, {
          method: 'PATCH',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ isRead: false }),
        }).then(() => { refetch(); refetchSmart(); }).catch(() => undefined);
      },
    },
    {
      label: 'Flag',
      action: () => {
        fetch(`/api/v1/messages/${msg.id}`, {
          method: 'PATCH',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ isFlagged: true }),
        }).then(() => { refetch(); refetchSmart(); }).catch(() => undefined);
      },
    },
    {
      label: 'Unflag',
      action: () => {
        fetch(`/api/v1/messages/${msg.id}`, {
          method: 'PATCH',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ isFlagged: false }),
        }).then(() => { refetch(); refetchSmart(); }).catch(() => undefined);
      },
    },
  ], [selectedMessageId, setSelectedMessage, refetch, refetchSmart]);

  // ── Derived values ───────────────────────────────────────────────────────

  const unreadCount = messages.filter((m) => !m.isRead).length;
  const allSelected = messages.length > 0 && messages.every((m) => selectedMessageIds.includes(m.id));
  const someSelected = selectedMessageIds.length > 0 && !allSelected;

  const smartLabels: Record<string, string> = { all: 'All Inboxes', unread: 'Unread', flagged: 'Flagged', snoozed: 'Snoozed' };
  const headerLabel = isAdvancedSearching
    ? 'Advanced search results'
    : isSearching
      ? `Results for "${debouncedQuery}"`
      : isSmartFolder
        ? (smartLabels[smartKind] ?? smartKind)
        : isLabelFolder
          ? (labels.find((l) => l.id === labelFolderId)?.name ?? 'Label')
          : selectedFolderId
            ? (folders.find((f) => f.id === selectedFolderId)?.name ?? 'Messages')
            : 'Select a folder';

  // Thread grouping only in regular folder view (not search / smart / label folders).
  const useThreading = !isSearching && !isAdvancedSearching && !isSmartFolder && !isLabelFolder && !!selectedFolderId;
  const threadGroups = useThreading && messages.length ? groupByThread(messages) : null;

  const toggleThread = (key: string) => {
    setExpandedThreads((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  // ── Render ───────────────────────────────────────────────────────────────

  return (
    <div className={styles.messageList}>
      {bulkError && <div role="alert" className={styles.bulkError}>{bulkError}</div>}
      {/* Bulk action bar */}
      {selectedMessageIds.length > 0 ? (
        <div className={styles.bulkBar}>
          <input
            type="checkbox"
            className={styles.selectAllCheckbox}
            checked={allSelected}
            ref={(el) => { if (el) el.indeterminate = someSelected; }}
            onChange={() => {
              if (allSelected) clearMessageSelection();
              else selectAllMessages(messages.map((m) => m.id));
            }}
            aria-label="Select all messages"
          />
          <span className={styles.bulkCount}>{selectedMessageIds.length} selected</span>
          <div className={styles.bulkActions}>
            <button type="button" className={styles.bulkBtn} onClick={() => handleBulkAction('archive')}>
              Archive
            </button>
            <button type="button" className={styles.bulkBtn} onClick={() => handleBulkAction('mark_read')}>
              Mark Read
            </button>
            <button type="button" className={styles.bulkBtn} onClick={() => handleBulkAction('mark_unread')}>
              Mark Unread
            </button>
            <button type="button" className={`${styles.bulkBtn} ${styles.bulkBtnDanger}`} onClick={() => handleBulkAction('delete')}>
              Delete
            </button>
          </div>
          <button
            type="button"
            className={styles.bulkCancel}
            onClick={clearMessageSelection}
            aria-label="Cancel selection"
          >
            ×
          </button>
        </div>
      ) : (
        <div className={styles.header}>
          <input
            type="checkbox"
            className={styles.selectAllCheckbox}
            checked={allSelected}
            ref={(el) => { if (el) el.indeterminate = someSelected; }}
            onChange={() => {
              if (allSelected) clearMessageSelection();
              else selectAllMessages(messages.map((m) => m.id));
            }}
            aria-label="Select all messages"
          />
          <span className={styles.folderName}>{headerLabel}</span>
          {!isSearching && unreadCount > 0 && (
            <span className={styles.unreadBadge}>{unreadCount} unread</span>
          )}
          {isSearching && messages.length > 0 && (
            <span className={styles.unreadBadge}>{messages.length}</span>
          )}
          {!isSearching && !isAdvancedSearching && selectedFolderId && !isSmartFolder && !isLabelFolder && unreadCount > 0 && (
            <button
              type="button"
              className={styles.markAllReadBtn}
              title="Mark all as read"
              onClick={() => {
                fetch(`/api/v1/folders/${selectedFolderId}/mark-read`, { method: 'POST' })
                  .then(() => refetch())
                  .catch(() => undefined);
              }}
            >
              Mark all read
            </button>
          )}
        </div>
      )}

      <div className={styles.list} ref={listRef}>
        {!isSearching && !selectedFolderId && (
          <div className={styles.empty}>No folder selected</div>
        )}
        {loading && <div className={styles.empty}>Loading…</div>}
        {!loading && isSearching && debouncedQuery.trim() && messages.length === 0 && (
          <div className={styles.empty}>No results for "{debouncedQuery}"</div>
        )}
        {!loading && !isSearching && (selectedFolderId || isLabelFolder) && messages.length === 0 && (
          <div className={styles.empty}>No messages</div>
        )}

        {/* Thread view */}
        {threadGroups?.map(({ key, latest, messages: threadMsgs }) => {
          const isExpanded = expandedThreads.has(key);
          const hasMany = threadMsgs.length > 1;
          return (
            <div key={key}>
              <MessageRow
                message={latest}
                isSelected={latest.id === selectedMessageId}
                isChecked={selectedMessageIds.includes(latest.id)}
                threadCount={hasMany ? threadMsgs.length : undefined}
                onClick={() => {
                  setSelectedMessage(latest.id);
                  if (hasMany && !isExpanded) toggleThread(key);
                }}
                onCheck={(e) => { e.stopPropagation(); toggleMessageSelection(latest.id); }}
                onContextMenu={(e) => openContextMenu(e, buildRowContextMenu(latest))}
              />
              {hasMany && isExpanded && (
                <>
                  {threadMsgs.slice(1).map((msg) => (
                    <MessageRow
                      key={msg.id}
                      message={msg}
                      isSelected={msg.id === selectedMessageId}
                      isChecked={selectedMessageIds.includes(msg.id)}
                      indent
                      onClick={() => setSelectedMessage(msg.id)}
                      onCheck={(e) => { e.stopPropagation(); toggleMessageSelection(msg.id); }}
                      onContextMenu={(e) => openContextMenu(e, buildRowContextMenu(msg))}
                    />
                  ))}
                  <button
                    type="button"
                    className={styles.threadExpandBtn}
                    onClick={() => toggleThread(key)}
                  >
                    ▲ Collapse thread
                  </button>
                </>
              )}
            </div>
          );
        })}

        {/* Flat view (search / smart folders) */}
        {!threadGroups && messages.map((message) => (
          <MessageRow
            key={message.id}
            message={message}
            isSelected={message.id === selectedMessageId}
            isChecked={selectedMessageIds.includes(message.id)}
            onClick={() => setSelectedMessage(message.id)}
            onCheck={(e) => { e.stopPropagation(); toggleMessageSelection(message.id); }}
            onContextMenu={(e) => openContextMenu(e, buildRowContextMenu(message))}
          />
        ))}
        {!loading && hasMore && !isSearching && !isAdvancedSearching && (
          <button
            type="button"
            className={styles.threadExpandBtn}
            onClick={() => setPage((current) => current + 1)}
          >
            Load more messages
          </button>
        )}
      </div>

      {contextMenu}
    </div>
  );
}
