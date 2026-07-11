import { useState } from 'react';
import styles from './ProfileScreen.module.css';
import { useAppStore } from '../store';
import { AccountSettingsScreen } from './AccountSettingsScreen';
import { logout } from '../api/client';

function ChevronRight() {
  return (
    <svg viewBox="0 0 8 14" fill="none" stroke="currentColor" strokeWidth="1.8" width="7" height="12"
      strokeLinecap="round" strokeLinejoin="round">
      <path d="M1 1l6 6-6 6" />
    </svg>
  );
}

export function ProfileScreen() {
  const { accounts, theme, setTheme } = useAppStore();
  const [settingsAccountId, setSettingsAccountId] = useState<string | null>(null);

  if (settingsAccountId) {
    return (
      <AccountSettingsScreen
        accountId={settingsAccountId}
        onClose={() => setSettingsAccountId(null)}
      />
    );
  }

  return (
    <div className={styles.screen}>
      <header className={styles.header}>
        <span className={styles.title}>Account</span>
      </header>

      <div className={`${styles.body} scroll`}>
        {/* Account cards — tap to open settings */}
        <div className={styles.group}>
          <div className={styles.groupLabel}>Accounts</div>
          {accounts.map((acc) => {
            const hue = acc.name.split('').reduce((n, c) => n + c.charCodeAt(0), 0) % 360;
            return (
              <button
                key={acc.id}
                className={styles.accountRow}
                onClick={() => setSettingsAccountId(acc.id)}
              >
                <div className={styles.accountAvatar} style={{ background: `oklch(62% 0.12 ${hue})` }}>
                  {acc.name.split(' ').map((w) => w[0]).join('').slice(0, 2).toUpperCase()}
                </div>
                <div className={styles.accountInfo}>
                  <span className={styles.accountName}>{acc.name}</span>
                  <span className={styles.accountEmail}>{acc.email}</span>
                </div>
                <span className={styles.chevron}><ChevronRight /></span>
              </button>
            );
          })}
        </div>

        {/* Appearance */}
        <div className={styles.group}>
          <div className={styles.groupLabel}>Appearance</div>
          <div className={styles.row}>
            <span className={styles.rowLabel}>Theme</span>
            <div className={styles.toggle}>
              <button
                className={`${styles.toggleOption} ${theme === 'light' ? styles.toggleActive : ''}`}
                onClick={() => setTheme('light')}
              >Light</button>
              <button
                className={`${styles.toggleOption} ${theme === 'dark' ? styles.toggleActive : ''}`}
                onClick={() => setTheme('dark')}
              >Dark</button>
            </div>
          </div>
        </div>

        <div className={styles.group}>
          <div className={styles.groupLabel}>Server</div>
          <button className={styles.accountRow} onClick={async () => {
            await logout().catch(() => undefined);
            window.location.reload();
          }}>
            <span className={styles.accountName}>Log out or switch server</span>
            <span className={styles.chevron}><ChevronRight /></span>
          </button>
        </div>

        <div className={styles.group}>
          <div className={styles.groupLabel}>About</div>
          <div className={styles.row}>
            <span className={styles.rowLabel}>rsMail</span>
            <span className={styles.rowValue}>v0.1.0</span>
          </div>
        </div>

        <div className={styles.bottomPad} />
      </div>
    </div>
  );
}
