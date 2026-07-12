import { useEffect, useState, useCallback, useRef } from 'react';
import styles from './InboxScreen.module.css';
import { MailRow } from '../components/MailRow';
import { FolderDrawer } from '../components/FolderDrawer';
import { useAppStore } from '../store';
import { api } from '../api/client';
import type { Account, Folder, Message } from '../types';

function getInbox(folders: Folder[]): Folder | null {
  return (
    folders.find((f) => f.specialUse?.toLowerCase() === 'inbox') ??
    folders.find((f) => f.name.toLowerCase() === 'inbox') ??
    null
  );
}

export function InboxScreen() {
  const { accounts, setAccounts, folders, setFolders, selectedFolderId, setSelectedFolder, setSelectedMessage, theme } = useAppStore();

  const [messages, setMessages] = useState<Message[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(false);

  // Search state
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<Message[]>([]);
  const [searchLoading, setSearchLoading] = useState(false);
  const [searchFocused, setSearchFocused] = useState(false);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  const isSearching = searchFocused || searchQuery.length > 0;

  // Load accounts + folders on mount
  useEffect(() => {
    (async () => {
      try {
        const accs = await api.accounts() as Account[];
        setAccounts(accs);
        if (accs.length === 0) { setLoading(false); return; }
        const allFolders = (await Promise.all(accs.map((a) => api.folders(a.id) as Promise<Folder[]>))).flat();
        setFolders(allFolders);
        const inbox = getInbox(allFolders);
        if (inbox) setSelectedFolder(inbox.id);
        else setLoading(false);
      } catch { setLoading(false); }
    })();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const loadMessages = useCallback(async (folderId: string, requestedPage = 1) => {
    setLoading(true);
    try {
      const msgs = await api.messages(folderId, requestedPage) as Message[];
      setMessages((current) => requestedPage === 1 ? msgs : [
        ...current,
        ...msgs.filter((message) => !current.some((existing) => existing.id === message.id)),
      ]);
      setPage(requestedPage);
      setHasMore(msgs.length === 40);
    } catch { /* ignore */ }
    setLoading(false);
  }, []);

  useEffect(() => {
    if (!selectedFolderId) return;
    loadMessages(selectedFolderId, 1);
  }, [selectedFolderId, loadMessages]);

  const handleSearchChange = (q: string) => {
    setSearchQuery(q);
    if (searchTimer.current) clearTimeout(searchTimer.current);
    if (!q.trim()) { setSearchResults([]); return; }
    searchTimer.current = setTimeout(async () => {
      setSearchLoading(true);
      try {
        const r = await api.search(q) as { messages: Message[] };
        setSearchResults(r.messages ?? []);
      } catch { setSearchResults([]); }
      setSearchLoading(false);
    }, 300);
  };

  const cancelSearch = () => {
    setSearchQuery('');
    setSearchResults([]);
    setSearchFocused(false);
    searchInputRef.current?.blur();
  };

  const handleRefresh = async () => {
    if (!selectedFolderId || refreshing) return;
    setRefreshing(true);
    await loadMessages(selectedFolderId, 1);
    setRefreshing(false);
  };

  const activeFolder = folders.find((f) => f.id === selectedFolderId);
  const unread = messages.filter((m) => !m.isRead).length;

  // Which list to show
  const listMessages = isSearching ? searchResults : messages;
  const listLoading  = isSearching ? searchLoading  : loading;

  return (
    <div className={styles.screen}>
      {/* Header — hidden while searching */}
      {!isSearching && (
        <header className={styles.header}>
          <button className={styles.folderBtn} onClick={() => setDrawerOpen(true)} aria-label="Browse folders">
            <svg viewBox="0 0 20 16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" width="20" height="16">
              <path d="M1 3h18M1 8h18M1 13h18" />
            </svg>
          </button>
          <div className={styles.headerCenter}>
            <span className={styles.folderName}>{activeFolder?.name ?? 'Mail'}</span>
            {unread > 0 && <span className={styles.badge}>{unread} unread</span>}
          </div>
          <button className={styles.headerBtn} onClick={handleRefresh} aria-label="Refresh">
            <img src={theme === 'dark' ? '/icons/refresh-dark.png' : '/icons/refresh-light.png'} alt="" width="20" height="20" />
          </button>
        </header>
      )}

      {/* Search bar */}
      <div className={`${styles.searchRow} ${isSearching ? styles.searchRowActive : ''}`}>
        <div className={styles.searchBox}>
          <img src={theme === 'dark' ? '/icons/search-dark.png' : '/icons/search-light.png'} alt="" width="16" height="16" className={styles.searchIcon} />
          <input
            ref={searchInputRef}
            className={styles.searchInput}
            type="search"
            placeholder="Search mail…"
            value={searchQuery}
            onChange={(e) => handleSearchChange(e.target.value)}
            onFocus={() => setSearchFocused(true)}
          />
          {searchQuery && (
            <button className={styles.clearBtn} onClick={() => handleSearchChange('')} aria-label="Clear">
              <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" width="12" height="12">
                <path d="M1 1l10 10M11 1L1 11" />
              </svg>
            </button>
          )}
        </div>
        {isSearching && (
          <button className={styles.cancelBtn} onClick={cancelSearch}>Cancel</button>
        )}
      </div>

      {/* Message / search list */}
      <div className={`${styles.list} scroll`}>
        {listLoading && <div className={styles.empty}>Loading…</div>}

        {!listLoading && isSearching && !searchQuery && (
          <div className={styles.empty}>Type to search your mail</div>
        )}

        {!listLoading && isSearching && searchQuery && listMessages.length === 0 && (
          <div className={styles.empty}>No results for "{searchQuery}"</div>
        )}

        {!listLoading && !isSearching && listMessages.length === 0 && (
          <div className={styles.empty}>No messages</div>
        )}

        {!listLoading && listMessages.map((msg) => (
          <MailRow key={msg.id} message={msg} onClick={() => setSelectedMessage(msg)} />
        ))}
        {!isSearching && !listLoading && hasMore && selectedFolderId && (
          <button
            type="button"
            onClick={() => loadMessages(selectedFolderId, page + 1)}
            style={{ width: 'calc(100% - 32px)', margin: 16, padding: 12 }}
          >
            Load more messages
          </button>
        )}
        <div className={styles.listPad} />
      </div>

      {/* Folder drawer */}
      {drawerOpen && (
        <FolderDrawer
          folders={folders}
          selectedId={selectedFolderId}
          onSelect={setSelectedFolder}
          onClose={() => setDrawerOpen(false)}
        />
      )}
    </div>
  );
}
