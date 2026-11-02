import { useState, useEffect, useCallback } from 'react';
import { api, reportApiError } from '../../api/client';
import styles from './SettingsModal.module.css';
import { useAppStore } from '../../store';
import { DENSITY_MIN, DENSITY_MAX, DENSITY_LABELS } from '../../utils/density';
import { RulesTab } from './RulesTab';
import type { Account, AccountSettings, Folder } from '../../types';

interface Webhook {
  id: string;
  url: string;
  secret: string | null;
  events: string;
  accountId: string | null;
  enabled: boolean;
}

// ── Global Settings Tab ───────────────────────────────────────────────────────

function GlobalTab() {
  const { theme, densityLevel, setTheme, setDensity } = useAppStore();

  return (
    <div>
      <div className={styles.section}>
        <div className={styles.sectionTitle}>Appearance</div>

        <div className={styles.fieldRow} style={{ marginBottom: 14 }}>
          <span className={styles.label}>Theme</span>
          <select
            className={styles.select}
            value={theme}
            onChange={(e) => setTheme(e.target.value as 'light' | 'dark')}
          >
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </div>

        <div className={styles.fieldRow}>
          <span className={styles.label}>Density</span>
          <div className={styles.stepper}>
            <button
              type="button"
              className={styles.stepBtn}
              onClick={() => setDensity(densityLevel - 1)}
              disabled={densityLevel <= DENSITY_MIN}
              aria-label="Denser"
            >−</button>
            <span className={styles.stepLabel}>{DENSITY_LABELS[densityLevel]}</span>
            <button
              type="button"
              className={styles.stepBtn}
              onClick={() => setDensity(densityLevel + 1)}
              disabled={densityLevel >= DENSITY_MAX}
              aria-label="More spacious"
            >+</button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Per-Account Settings Tab ──────────────────────────────────────────────────

interface AccountTabProps {
  account: Account;
  folders: Folder[];
  onDeleted: () => void;
}

function AccountTab({ account, folders, onDeleted }: AccountTabProps) {
  const [settings, setSettings] = useState<AccountSettings | null>(null);
  const [name, setName] = useState('');
  const [syncDays, setSyncDays] = useState('');
  const [signature, setSignature] = useState('');
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<{ msg: string; ok: boolean } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);

  useEffect(() => {
    api.get<AccountSettings>(`/api/v1/accounts/${account.id}/settings`)
      .then((s) => {
        setSettings(s);
        setName(s.name);
        setSyncDays(s.syncDaysLimit != null ? String(s.syncDaysLimit) : '');
        setSignature(s.signature ?? '');
      })
      .catch((e) => {
        reportApiError(e);
        setName(account.name);
      });
  }, [account.id, account.name]);

  const handleSave = useCallback(async () => {
    setSaving(true);
    setStatus(null);
    try {
      const body: Record<string, unknown> = { name };
      if (syncDays.trim()) body.syncDaysLimit = parseInt(syncDays, 10);
      else body.syncDaysLimit = null;
      body.signature = signature || null;

      await api.patch(`/api/v1/accounts/${account.id}`, body, { silent: true });
      setStatus({ msg: 'Saved', ok: true });
    } catch (e) {
      setStatus({ msg: e instanceof Error ? `Save failed: ${e.message}` : 'Save failed', ok: false });
    } finally {
      setSaving(false);
    }
  }, [account.id, name, syncDays, signature]);

  const handleDelete = useCallback(async () => {
    try {
      await api.delete(`/api/v1/accounts/${account.id}`);
      onDeleted();
    } catch {
      // reported to the user by the API client
    }
  }, [account.id, onDeleted]);

  const toggleFolder = useCallback(async (folder: Folder) => {
    await api.patch(`/api/v1/folders/${folder.id}`, { isExcluded: !folder.isExcluded });
    // Optimistically reflect the change; parent can re-fetch later.
    folder.isExcluded = !folder.isExcluded;
  }, []);

  return (
    <div>
      <div className={styles.section}>
        <div className={styles.sectionTitle}>Account Info</div>
        <div className={styles.fieldRow} style={{ marginBottom: 14 }}>
          <span className={styles.label}>Display name</span>
          <input
            className={styles.input}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </div>
        <div className={styles.fieldRow} style={{ marginBottom: 14 }}>
          <span className={styles.label}>Email</span>
          <span style={{ fontSize: 13, color: 'var(--text-mid)' }}>{account.email}</span>
        </div>
        <div className={styles.fieldRow} style={{ marginBottom: 14 }}>
          <span className={styles.label}>Provider</span>
          <span style={{ fontSize: 13, color: 'var(--text-mid)' }}>{settings?.providerType ?? account.providerType}</span>
        </div>
      </div>

      <div className={styles.section}>
        <div className={styles.sectionTitle}>Sync Settings</div>
        <div className={styles.fieldRow} style={{ marginBottom: 14 }}>
          <span className={styles.label}>Download last N days</span>
          <input
            className={styles.input}
            type="number"
            min="1"
            placeholder="All (no limit)"
            value={syncDays}
            onChange={(e) => setSyncDays(e.target.value)}
            style={{ maxWidth: 140 }}
          />
        </div>
      </div>

      <div className={styles.section}>
        <div className={styles.sectionTitle}>Signature</div>
        <textarea
          className={styles.textarea}
          value={signature}
          onChange={(e) => setSignature(e.target.value)}
          placeholder="Your email signature…"
          style={{ width: '100%', boxSizing: 'border-box' }}
        />
      </div>

      <div className={styles.section}>
        <div className={styles.sectionTitle}>Excluded Folders</div>
        <p style={{ fontSize: 13, color: 'var(--text-mid)', marginBottom: 10 }}>
          Excluded folders are skipped during sync. Toggle to include or exclude.
        </p>
        <div className={styles.folderList}>
          {folders.map((folder) => (
            <label key={folder.id} className={styles.folderItem}>
              <input
                type="checkbox"
                checked={!folder.isExcluded}
                onChange={() => toggleFolder(folder)}
              />
              <span className={styles.folderName}>{folder.name}</span>
              {folder.isExcluded && (
                <span className={styles.folderExcluded}>excluded</span>
              )}
            </label>
          ))}
          {folders.length === 0 && (
            <div style={{ padding: '8px', fontSize: 13, color: 'var(--text-dim)' }}>
              No folders synced yet.
            </div>
          )}
        </div>
      </div>

      <div className={styles.section}>
        <div className={styles.sectionTitle}>Danger Zone</div>
        {!confirmDelete ? (
          <button
            className={styles.dangerBtn}
            type="button"
            onClick={() => setConfirmDelete(true)}
          >
            Remove account
          </button>
        ) : (
          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <span style={{ fontSize: 13, color: 'var(--text)' }}>
              Delete <strong>{account.email}</strong> and all its cached emails?
            </span>
            <button className={styles.dangerBtn} type="button" onClick={handleDelete}>
              Yes, delete
            </button>
            <button
              style={{ background: 'none', border: 'none', fontSize: 13, cursor: 'pointer', color: 'var(--text-mid)' }}
              type="button"
              onClick={() => setConfirmDelete(false)}
            >
              Cancel
            </button>
          </div>
        )}
      </div>

      <div className={styles.footer}>
        {status && (
          <span className={`${styles.statusMsg} ${status.ok ? styles.ok : styles.err}`}>
            {status.msg}
          </span>
        )}
        <button className={styles.saveBtn} type="button" onClick={handleSave} disabled={saving}>
          {saving ? 'Saving…' : 'Save changes'}
        </button>
      </div>
    </div>
  );
}

// ── Webhooks Tab ─────────────────────────────────────────────────────────────

function WebhooksTab() {
  const [webhooks, setWebhooks] = useState<Webhook[]>([]);
  const [newUrl, setNewUrl] = useState('');
  const [newSecret, setNewSecret] = useState('');
  const [newEvents, setNewEvents] = useState('new_message');
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    api.get<Webhook[]>('/api/v1/webhooks')
      .then((data) => setWebhooks(data))
      .catch(reportApiError);
  }, []);

  useEffect(() => { load(); }, [load]);

  const handleAdd = useCallback(async () => {
    if (!newUrl.trim()) return;
    setAdding(true);
    setError(null);
    try {
      await api.post('/api/v1/webhooks', {
          url: newUrl.trim(),
          secret: newSecret.trim() || null,
          events: newEvents || 'new_message',
        }, { silent: true });
      setNewUrl('');
      setNewSecret('');
      load();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to add webhook');
    } finally {
      setAdding(false);
    }
  }, [newUrl, newSecret, newEvents, load]);

  const handleToggle = useCallback(async (wh: Webhook) => {
    await api.patch(`/api/v1/webhooks/${wh.id}`, { enabled: !wh.enabled });
    load();
  }, [load]);

  const handleDelete = useCallback(async (id: string) => {
    await api.delete(`/api/v1/webhooks/${id}`);
    load();
  }, [load]);

  return (
    <div>
      <div className={styles.section}>
        <div className={styles.sectionTitle}>Configured Webhooks</div>
        <p style={{ fontSize: 13, color: 'var(--text-mid)', marginBottom: 12 }}>
          Webhooks POST a JSON payload to your URL when events occur. Supports HMAC-SHA256 signing via
          <code style={{ fontSize: 12, background: 'var(--tag)', padding: '1px 5px', borderRadius: 3, marginLeft: 4 }}>X-Email-Signature</code>.
        </p>

        {webhooks.length === 0 && (
          <div style={{ fontSize: 13, color: 'var(--text-dim)', marginBottom: 16 }}>No webhooks configured.</div>
        )}

        {webhooks.map((wh) => (
          <div key={wh.id} style={{
            display: 'flex', alignItems: 'center', gap: 10,
            padding: '8px 10px', background: 'var(--tag)', borderRadius: 'var(--radius-sm)',
            marginBottom: 6,
          }}>
            <input
              type="checkbox"
              checked={wh.enabled}
              onChange={() => handleToggle(wh)}
              title={wh.enabled ? 'Disable' : 'Enable'}
            />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 13, color: 'var(--text)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {wh.url}
              </div>
              <div style={{ fontSize: 11, color: 'var(--text-dim)' }}>
                Events: {wh.events}{wh.secret ? ' · signed' : ''}
              </div>
            </div>
            <button
              type="button"
              onClick={() => handleDelete(wh.id)}
              style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-dim)', fontSize: 16, padding: '2px 6px' }}
            >
              ×
            </button>
          </div>
        ))}
      </div>

      <div className={styles.section}>
        <div className={styles.sectionTitle}>Add Webhook</div>
        <div className={styles.field}>
          <div className={styles.fieldRow} style={{ marginBottom: 10 }}>
            <span className={styles.label}>URL</span>
            <input
              className={styles.input}
              type="url"
              placeholder="https://example.com/webhook"
              value={newUrl}
              onChange={(e) => setNewUrl(e.target.value)}
            />
          </div>
          <div className={styles.fieldRow} style={{ marginBottom: 10 }}>
            <span className={styles.label}>Secret (optional)</span>
            <input
              className={styles.input}
              type="text"
              placeholder="HMAC signing secret"
              value={newSecret}
              onChange={(e) => setNewSecret(e.target.value)}
            />
          </div>
          <div className={styles.fieldRow} style={{ marginBottom: 14 }}>
            <span className={styles.label}>Events</span>
            <select
              className={styles.select}
              value={newEvents}
              onChange={(e) => setNewEvents(e.target.value)}
            >
              <option value="new_message">new_message</option>
              <option value="all">all</option>
            </select>
          </div>
        </div>
        {error && <div style={{ fontSize: 12, color: '#c0392b', marginBottom: 8 }}>{error}</div>}
        <div className={styles.footer}>
          <button
            className={styles.saveBtn}
            type="button"
            onClick={handleAdd}
            disabled={adding || !newUrl.trim()}
          >
            {adding ? 'Adding…' : 'Add webhook'}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Main Modal ────────────────────────────────────────────────────────────────

interface SettingsModalProps {
  onAccountDeleted: () => void;
}

export function SettingsModal({ onAccountDeleted }: SettingsModalProps) {
  const { accounts, closeSettings, folders } = useAppStore();
  const [activeTab, setActiveTab] = useState('global');

  const tabs = [
    { id: 'global', label: 'General' },
    ...accounts.map((a) => ({ id: a.id, label: a.name || a.email })),
    ...accounts.map((a) => ({ id: `rules-${a.id}`, label: `Rules (${a.name || a.email})` })),
    { id: 'webhooks', label: 'Webhooks' },
  ];

  const activeAccount = accounts.find((a) => a.id === activeTab);
  const accountFolders = activeAccount
    ? folders.filter((f) => f.accountId === activeAccount.id)
    : [];

  return (
    <div className={styles.overlay} onClick={(e) => { if (e.target === e.currentTarget) closeSettings(); }}>
      <div className={styles.modal}>
        <div className={styles.header}>
          <span className={styles.title}>Settings</span>
          <button className={styles.closeBtn} type="button" onClick={closeSettings}>×</button>
        </div>

        <div className={styles.tabs}>
          {tabs.map((tab) => (
            <button
              key={tab.id}
              type="button"
              className={`${styles.tab}${activeTab === tab.id ? ` ${styles.active}` : ''}`}
              onClick={() => setActiveTab(tab.id)}
            >
              {tab.label}
            </button>
          ))}
        </div>

        <div className={styles.body}>
          {activeTab === 'global' ? (
            <GlobalTab />
          ) : activeTab === 'webhooks' ? (
            <WebhooksTab />
          ) : activeTab.startsWith('rules-') ? (
            <RulesTab accountId={activeTab.slice('rules-'.length)} />
          ) : activeAccount ? (
            <AccountTab
              account={activeAccount}
              folders={accountFolders}
              onDeleted={() => {
                closeSettings();
                onAccountDeleted();
              }}
            />
          ) : null}
        </div>
      </div>
    </div>
  );
}
