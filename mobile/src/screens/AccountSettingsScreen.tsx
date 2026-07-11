import { useEffect, useState } from 'react';
import styles from './AccountSettingsScreen.module.css';
import { useAppStore } from '../store';
import { api } from '../api/client';
import type { AccountSettings, Folder } from '../types';

const SYNC_OPTIONS: { label: string; value: number | null }[] = [
  { label: '7 days',   value: 7 },
  { label: '30 days',  value: 30 },
  { label: '60 days',  value: 60 },
  { label: '90 days',  value: 90 },
  { label: '180 days', value: 180 },
  { label: '1 year',   value: 365 },
  { label: 'All mail', value: null },
];

interface Props {
  accountId: string;
  onClose: () => void;
}

export function AccountSettingsScreen({ accountId, onClose }: Props) {
  const { folders, setFolders } = useAppStore();
  const [settings, setSettings] = useState<AccountSettings | null>(null);
  const [saving, setSaving] = useState(false);
  const [showFolders, setShowFolders] = useState(false);

  const accountFolders = folders.filter((f) => f.accountId === accountId);

  useEffect(() => {
    api.accountSettings(accountId)
      .then((s) => setSettings(s as AccountSettings))
      .catch(() => { /* ignore */ });
  }, [accountId]);

  const setSyncDays = async (value: number | null) => {
    if (!settings) return;
    setSaving(true);
    await api.updateAccount(accountId, { syncDaysLimit: value });
    setSettings({ ...settings, syncDaysLimit: value });
    setSaving(false);
  };

  const toggleExclude = async (folder: Folder) => {
    const next = !folder.isExcluded;
    await api.updateFolder(folder.id, { isExcluded: next });
    setFolders(folders.map((f) => f.id === folder.id ? { ...f, isExcluded: next } : f));
  };

  return (
    <div className={styles.screen}>
      <header className={styles.header}>
        <button className={styles.back} onClick={onClose} aria-label="Back">
          <svg viewBox="0 0 10 16" fill="none" stroke="currentColor" strokeWidth="2" width="10" height="16" strokeLinecap="round" strokeLinejoin="round">
            <path d="M8 1L2 8l6 7" />
          </svg>
        </button>
        <span className={styles.title}>Account Settings</span>
        <div style={{ width: 36 }} />
      </header>

      <div className={`${styles.body} scroll`}>
        {settings && (
          <div className={styles.accountCard}>
            <div className={styles.accountName}>{settings.name}</div>
            <div className={styles.accountEmail}>{settings.email}</div>
          </div>
        )}

        {/* Sync period */}
        <div className={styles.group}>
          <div className={styles.groupLabel}>Download last</div>
          {SYNC_OPTIONS.map((opt) => {
            const active = settings?.syncDaysLimit === opt.value;
            return (
              <button
                key={String(opt.value)}
                className={`${styles.optionRow} ${active ? styles.optionActive : ''}`}
                onClick={() => setSyncDays(opt.value)}
                disabled={saving}
              >
                <span className={styles.optionLabel}>{opt.label}</span>
                {active && (
                  <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="2" width="14" height="14" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M1 6l4 4 6-8" />
                  </svg>
                )}
              </button>
            );
          })}
        </div>

        {/* Folder exclusions */}
        <div className={styles.group}>
          <button className={styles.sectionToggle} onClick={() => setShowFolders((v) => !v)}>
            <span className={styles.groupLabel} style={{ margin: 0 }}>Exclude folders from sync</span>
            <svg
              viewBox="0 0 10 6" fill="none" stroke="currentColor" strokeWidth="1.8" width="12" height="8"
              strokeLinecap="round" strokeLinejoin="round"
              style={{ transform: showFolders ? 'rotate(180deg)' : 'none', transition: 'transform 150ms' }}
            >
              <path d="M1 1l4 4 4-4" />
            </svg>
          </button>

          {showFolders && accountFolders.map((folder) => (
            <div key={folder.id} className={styles.folderRow}>
              <div className={styles.folderInfo}>
                <span className={styles.folderName}>{folder.fullPath}</span>
                {folder.unreadCount > 0 && (
                  <span className={styles.folderCount}>{folder.unreadCount}</span>
                )}
              </div>
              <button
                className={`${styles.toggle} ${folder.isExcluded ? styles.toggleOn : ''}`}
                onClick={() => toggleExclude(folder)}
                aria-label={folder.isExcluded ? 'Include in sync' : 'Exclude from sync'}
              >
                <span className={styles.toggleThumb} />
              </button>
            </div>
          ))}
        </div>

        <div className={styles.bottomPad} />
      </div>
    </div>
  );
}
