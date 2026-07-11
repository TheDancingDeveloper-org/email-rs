import { useEffect, useState } from 'react';
import { useAppStore } from './store';
import { BottomNav } from './components/BottomNav';
import { InboxScreen } from './screens/InboxScreen';
import { ProfileScreen } from './screens/ProfileScreen';
import { MailDetailScreen } from './screens/MailDetailScreen';
import { ComposeScreen } from './screens/ComposeScreen';
import { ServerLoginScreen } from './screens/ServerLoginScreen';
import { clearSession, loadSession, testConnection } from './api/client';

export function App() {
  const [auth, setAuth] = useState<'loading' | 'authenticated' | 'anonymous'>('loading');
  const { screen, theme, selectedMessage, composeOpen } = useAppStore();

  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme);
  }, [theme]);

  useEffect(() => {
    let active = true;
    loadSession()
      .then(async value => {
        if (!value) return false;
        try { await testConnection(value); return true; }
        catch { await clearSession(); return false; }
      })
      .then(ok => { if (active) setAuth(ok ? 'authenticated' : 'anonymous'); })
      .catch(() => { if (active) setAuth('anonymous'); });
    return () => { active = false; };
  }, []);

  if (auth === 'loading') return <main style={{ padding: 24 }}>Connecting…</main>;
  if (auth === 'anonymous') return <ServerLoginScreen onAuthenticated={() => setAuth('authenticated')} />;

  return (
    <div style={{ height: '100%', position: 'relative', background: 'var(--bg)' }}>
      {/* Active screen */}
      {screen === 'inbox'    && <InboxScreen />}
      {screen === 'profile'  && <ProfileScreen />}
      {screen === 'calendar' && <InboxScreen />}  {/* placeholder */}

      {/* Bottom nav — always visible except when detail/compose is open */}
      {!selectedMessage && !composeOpen && <BottomNav />}

      {/* Overlays */}
      {selectedMessage && <MailDetailScreen />}
      {composeOpen     && <ComposeScreen />}
    </div>
  );
}
