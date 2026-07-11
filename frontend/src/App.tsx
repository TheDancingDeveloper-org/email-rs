import React, { useCallback, useEffect, useRef, useState } from 'react';
import { Sidebar } from './components/Sidebar/Sidebar';
import { MessageList } from './components/MessageList/MessageList';
import { ReadingPane } from './components/ReadingPane/ReadingPane';
import { AccountSetup } from './components/AccountSetup/AccountSetup';
import { ComposeModal } from './components/Compose/ComposeModal';
import { SettingsModal } from './components/Settings/SettingsModal';
import { CommandPalette } from './components/CommandPalette/CommandPalette';
import { CalendarView } from './components/Calendar/CalendarView';
import { useAppStore } from './store';
import { applyDensity } from './utils/density';
import type { Account } from './types';

const SIDEBAR_W_KEY = 'email_sidebar_w';
const MSGLIST_W_KEY  = 'email_msglist_w';
const DEFAULT_SIDEBAR_W = 200;
const DEFAULT_MSGLIST_W = 280;
const MIN_W = 140;
const MAX_SIDEBAR_W = 480;
const MAX_MSGLIST_W = 600;

function AuthGate({ onAuthenticated }: { onAuthenticated: () => void }) {
  const [username, setUsername] = useState('admin');
  const [password, setPassword] = useState('');
  const [firstRun, setFirstRun] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      const response = await fetch(`/api/v1/app-auth/${firstRun ? 'bootstrap' : 'login'}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username, password, client: 'cookie' }),
      });
      if (!response.ok) {
        const body = await response.json().catch(() => ({})) as { error?: string };
        throw new Error(body.error || `${response.status} ${response.statusText}`);
      }
      onAuthenticated();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Authentication failed');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div style={{ minHeight: '100vh', display: 'grid', placeItems: 'center', background: 'var(--bg)' }}>
      <form onSubmit={submit} style={{ width: 340, padding: 28, border: '1px solid var(--divider)', borderRadius: 10, background: 'var(--surface)' }}>
        <h1 style={{ margin: '0 0 6px', fontSize: 22, color: 'var(--text)' }}>email-rs</h1>
        <p style={{ margin: '0 0 22px', fontSize: 13, color: 'var(--text-dim)' }}>
          {firstRun ? 'Create the single application user. This is allowed only from the server host.' : 'Sign in to your mail server.'}
        </p>
        <label style={{ display: 'block', marginBottom: 12, fontSize: 12, color: 'var(--text-mid)' }}>
          Username
          <input value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="username" required
            style={{ display: 'block', boxSizing: 'border-box', width: '100%', marginTop: 5, padding: '9px 10px', border: '1px solid var(--divider)', borderRadius: 5, background: 'var(--bg)', color: 'var(--text)' }} />
        </label>
        <label style={{ display: 'block', marginBottom: 12, fontSize: 12, color: 'var(--text-mid)' }}>
          Password
          <input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete={firstRun ? 'new-password' : 'current-password'} required minLength={firstRun ? 12 : undefined}
            style={{ display: 'block', boxSizing: 'border-box', width: '100%', marginTop: 5, padding: '9px 10px', border: '1px solid var(--divider)', borderRadius: 5, background: 'var(--bg)', color: 'var(--text)' }} />
        </label>
        {error && <div role="alert" style={{ marginBottom: 12, fontSize: 12, color: 'var(--danger)' }}>{error}</div>}
        <button type="submit" disabled={submitting} style={{ width: '100%', padding: '9px 12px', border: 0, borderRadius: 5, background: 'var(--accent)', color: 'white', cursor: 'pointer' }}>
          {submitting ? 'Please wait…' : firstRun ? 'Create user' : 'Sign in'}
        </button>
        <button type="button" onClick={() => { setFirstRun(!firstRun); setError(null); }} style={{ width: '100%', marginTop: 10, border: 0, background: 'none', color: 'var(--text-dim)', cursor: 'pointer', fontSize: 12 }}>
          {firstRun ? 'Back to sign in' : 'First-run setup'}
        </button>
      </form>
    </div>
  );
}

function storedWidth(key: string, def: number): number {
  try {
    const v = localStorage.getItem(key);
    return v ? Math.max(MIN_W, Number(v)) : def;
  } catch { return def; }
}

function usePanelResize(key: string, def: number, min: number, max: number) {
  const [width, setWidth] = useState(() => storedWidth(key, def));
  const widthRef = useRef(width);
  useEffect(() => { widthRef.current = width; }, [width]);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = widthRef.current;

    const onMove = (ev: MouseEvent) => {
      setWidth(Math.min(max, Math.max(min, startW + (ev.clientX - startX))));
    };
    const onUp = (ev: MouseEvent) => {
      const final = Math.min(max, Math.max(min, startW + (ev.clientX - startX)));
      setWidth(final);
      try { localStorage.setItem(key, String(Math.round(final))); } catch { /* */ }
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };

    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
  }, [key, min, max]);

  return { width, onMouseDown };
}

const handleStyle: React.CSSProperties = {
  width: 4,
  flexShrink: 0,
  cursor: 'col-resize',
  background: 'transparent',
  position: 'relative',
  zIndex: 10,
  transition: 'background 0.15s',
};

export function App() {
  const { theme, densityLevel, accounts, setAccounts, folders, compose, settingsOpen, openPalette, closePalette, paletteOpen, view } = useAppStore();
  const [accountsLoaded, setAccountsLoaded] = useState(false);
  const [authRequired, setAuthRequired] = useState(false);

  const sidebar = usePanelResize(SIDEBAR_W_KEY,  DEFAULT_SIDEBAR_W, MIN_W, MAX_SIDEBAR_W);
  const msglist = usePanelResize(MSGLIST_W_KEY,  DEFAULT_MSGLIST_W, MIN_W, MAX_MSGLIST_W);

  useEffect(() => { document.documentElement.setAttribute('data-theme', theme); }, [theme]);
  useEffect(() => { applyDensity(densityLevel); }, [densityLevel]);

  // Reflect total unread count in the browser tab title.
  useEffect(() => {
    const total = folders.reduce((n, f) => n + (f.unreadCount ?? 0), 0);
    document.title = total > 0 ? `(${total}) email-rs` : 'email-rs';
  }, [folders]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
        e.preventDefault();
        paletteOpen ? closePalette() : openPalette();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [paletteOpen, openPalette, closePalette]);

  const fetchAccounts = () => {
    fetch('/api/v1/accounts')
      .then(async (res) => {
        if (res.status === 401) {
          setAuthRequired(true);
          setAccountsLoaded(true);
          return null;
        }
        if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
        return res.json() as Promise<Account[]>;
      })
      .then((data) => {
        if (!data) return;
        setAuthRequired(false);
        setAccounts(data);
        setAccountsLoaded(true);
      })
      .catch(() => { setAccounts([]); setAccountsLoaded(true); });
  };

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (params.get('oauth') === 'success') window.history.replaceState(null, '', window.location.pathname);
    fetchAccounts();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!accountsLoaded) return null;
  if (authRequired) return <AuthGate onAuthenticated={fetchAccounts} />;
  if (accounts.length === 0) return <AccountSetup onAccountAdded={fetchAccounts} />;

  return (
    <div style={{ display: 'flex', height: '100vh', overflow: 'hidden' }}>
        <div style={{ width: sidebar.width, flexShrink: 0, overflow: 'hidden', height: '100%' }}>
          <Sidebar onAccountAdded={fetchAccounts} />
        </div>

        <div
          style={handleStyle}
          onMouseDown={sidebar.onMouseDown}
          onMouseEnter={(e) => { (e.currentTarget as HTMLDivElement).style.background = 'var(--divider)'; }}
          onMouseLeave={(e) => { (e.currentTarget as HTMLDivElement).style.background = 'transparent'; }}
        />

        {view === 'calendar' ? (
          <div style={{ flex: 1, overflow: 'hidden', height: '100%', minWidth: 0 }}>
            <CalendarView />
          </div>
        ) : (
          <>
            <div style={{ width: msglist.width, flexShrink: 0, overflow: 'hidden', height: '100%' }}>
              <MessageList />
            </div>

            <div
              style={handleStyle}
              onMouseDown={msglist.onMouseDown}
              onMouseEnter={(e) => { (e.currentTarget as HTMLDivElement).style.background = 'var(--divider)'; }}
              onMouseLeave={(e) => { (e.currentTarget as HTMLDivElement).style.background = 'transparent'; }}
            />

            <div style={{ flex: 1, overflow: 'hidden', height: '100%', minWidth: 0 }}>
              {compose ? <ComposeModal /> : <ReadingPane />}
            </div>
          </>
        )}

      {settingsOpen && <SettingsModal onAccountDeleted={fetchAccounts} />}
      <CommandPalette />
    </div>
  );
}
