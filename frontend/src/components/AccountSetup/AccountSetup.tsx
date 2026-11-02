import React, { useState } from 'react';
import styles from './AccountSetup.module.css';
import { useAppStore } from '../../store';
import { apiPost } from '../../hooks/useApi';
import { api, listAccounts } from '../../api/client';
import type { Account } from '../../types';

// ── Microsoft icon ────────────────────────────────────────────────────────────

function MicrosoftIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
      <rect x="1" y="1" width="7.5" height="7.5" fill="#F25022" />
      <rect x="9.5" y="1" width="7.5" height="7.5" fill="#7FBA00" />
      <rect x="1" y="9.5" width="7.5" height="7.5" fill="#00A4EF" />
      <rect x="9.5" y="9.5" width="7.5" height="7.5" fill="#FFB900" />
    </svg>
  );
}

// ── Google G icon ─────────────────────────────────────────────────────────────

function GoogleGIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
      <path
        d="M17.64 9.2c0-.637-.057-1.251-.164-1.84H9v3.481h4.844a4.14 4.14 0 01-1.796 2.716v2.259h2.908c1.702-1.567 2.684-3.875 2.684-6.615z"
        fill="#4285F4"
      />
      <path
        d="M9 18c2.43 0 4.467-.806 5.956-2.18l-2.908-2.259c-.806.54-1.837.86-3.048.86-2.344 0-4.328-1.584-5.036-3.711H.957v2.332A8.997 8.997 0 009 18z"
        fill="#34A853"
      />
      <path
        d="M3.964 10.71A5.41 5.41 0 013.682 9c0-.593.102-1.17.282-1.71V4.958H.957A8.996 8.996 0 000 9c0 1.452.348 2.827.957 4.042l3.007-2.332z"
        fill="#FBBC05"
      />
      <path
        d="M9 3.58c1.321 0 2.508.454 3.44 1.345l2.582-2.58C13.463.891 11.426 0 9 0A8.997 8.997 0 00.957 4.958L3.964 6.29C4.672 4.163 6.656 3.58 9 3.58z"
        fill="#EA4335"
      />
    </svg>
  );
}

// ── Microsoft app-password form ───────────────────────────────────────────────

interface MicrosoftAppPasswordFormProps {
  onSuccess: () => void;
  styles: Record<string, string>;
}

function MicrosoftAppPasswordForm({ onSuccess, styles: s }: MicrosoftAppPasswordFormProps) {
  const { setAccounts } = useAppStore();
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [imapHost, setImapHost] = useState('outlook.office365.com');
  const [smtpHost, setSmtpHost] = useState('smtp.office365.com');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      await apiPost<Account>('/api/v1/accounts', {
        name,
        email,
        providerType: 'microsoft365',
        authType: 'app_password',
        host: imapHost,
        port: 993,
        password,
        smtpHost,
        smtpPort: 587,
        smtpPassword: password,
      });
      setAccounts(await listAccounts());
      onSuccess();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <form className={s.form} onSubmit={handleSubmit}>
      <div className={s.field}>
        <label className={s.label} htmlFor="ms-ap-name">Name</label>
        <input id="ms-ap-name" className={s.input} type="text" value={name}
          onChange={(e) => setName(e.target.value)} required autoComplete="name" />
      </div>
      <div className={s.field}>
        <label className={s.label} htmlFor="ms-ap-email">Work email</label>
        <input id="ms-ap-email" className={s.input} type="email" value={email}
          onChange={(e) => setEmail(e.target.value)} required autoComplete="email" />
      </div>
      <div className={s.field}>
        <label className={s.label} htmlFor="ms-ap-pw">App password</label>
        <input id="ms-ap-pw" className={s.input} type="password" value={password}
          onChange={(e) => setPassword(e.target.value)} required autoComplete="current-password" />
      </div>
      <div className={s.field}>
        <label className={s.label} htmlFor="ms-ap-imap">IMAP host</label>
        <input id="ms-ap-imap" className={s.input} type="text" value={imapHost}
          onChange={(e) => setImapHost(e.target.value)} required />
      </div>
      <div className={s.field}>
        <label className={s.label} htmlFor="ms-ap-smtp">SMTP host</label>
        <input id="ms-ap-smtp" className={s.input} type="text" value={smtpHost}
          onChange={(e) => setSmtpHost(e.target.value)} required />
      </div>
      {error && <p className={s.errorMsg}>{error}</p>}
      <button className={s.submitBtn} type="submit" disabled={submitting}>
        {submitting && <span className={s.spinner} />}
        {submitting ? 'Adding account…' : 'Add account'}
      </button>
    </form>
  );
}

// ── Manual IMAP/SMTP form ────────────────────────────────────────────────────

interface ManualFormProps {
  onSuccess: () => void;
  styles: Record<string, string>;
}

function ManualForm({ onSuccess, styles: s }: ManualFormProps) {
  const { setAccounts } = useAppStore();
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [imapHost, setImapHost] = useState('');
  const [imapPort, setImapPort] = useState('993');
  const [imapPassword, setImapPassword] = useState('');
  const [smtpHost, setSmtpHost] = useState('');
  const [smtpPort, setSmtpPort] = useState('587');
  const [samePassword, setSamePassword] = useState(true);
  const [smtpPassword, setSmtpPassword] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      await apiPost<Account>('/api/v1/accounts', {
        name,
        email,
        providerType: 'imap',
        authType: 'basic',
        host: imapHost,
        port: Number(imapPort),
        password: imapPassword,
        smtpHost,
        smtpPort: Number(smtpPort),
        smtpPassword: samePassword ? imapPassword : smtpPassword,
      });
      // Refetch accounts list from parent
      setAccounts(await listAccounts());
      onSuccess();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <form className={s.form} onSubmit={handleSubmit}>
      <div className={s.field}>
        <label className={s.label} htmlFor="m-name">Name</label>
        <input
          id="m-name"
          className={s.input}
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
          autoComplete="name"
        />
      </div>
      <div className={s.field}>
        <label className={s.label} htmlFor="m-email">Email</label>
        <input
          id="m-email"
          className={s.input}
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          required
          autoComplete="email"
        />
      </div>
      <div className={s.row}>
        <div className={s.field}>
          <label className={s.label} htmlFor="m-imap-host">IMAP host</label>
          <input
            id="m-imap-host"
            className={s.input}
            type="text"
            value={imapHost}
            onChange={(e) => setImapHost(e.target.value)}
            required
          />
        </div>
        <div className={s.field}>
          <label className={s.label} htmlFor="m-imap-port">Port</label>
          <input
            id="m-imap-port"
            className={s.input}
            type="number"
            value={imapPort}
            onChange={(e) => setImapPort(e.target.value)}
            required
          />
        </div>
      </div>
      <div className={s.field}>
        <label className={s.label} htmlFor="m-imap-pw">IMAP password</label>
        <input
          id="m-imap-pw"
          className={s.input}
          type="password"
          value={imapPassword}
          onChange={(e) => setImapPassword(e.target.value)}
          required
          autoComplete="current-password"
        />
      </div>
      <div className={s.row}>
        <div className={s.field}>
          <label className={s.label} htmlFor="m-smtp-host">SMTP host</label>
          <input
            id="m-smtp-host"
            className={s.input}
            type="text"
            value={smtpHost}
            onChange={(e) => setSmtpHost(e.target.value)}
            required
          />
        </div>
        <div className={s.field}>
          <label className={s.label} htmlFor="m-smtp-port">Port</label>
          <input
            id="m-smtp-port"
            className={s.input}
            type="number"
            value={smtpPort}
            onChange={(e) => setSmtpPort(e.target.value)}
            required
          />
        </div>
      </div>
      <label className={s.checkboxRow}>
        <input
          type="checkbox"
          checked={samePassword}
          onChange={(e) => setSamePassword(e.target.checked)}
        />
        Use same password for SMTP
      </label>
      {!samePassword && (
        <div className={s.field}>
          <label className={s.label} htmlFor="m-smtp-pw">SMTP password</label>
          <input
            id="m-smtp-pw"
            className={s.input}
            type="password"
            value={smtpPassword}
            onChange={(e) => setSmtpPassword(e.target.value)}
            required
            autoComplete="current-password"
          />
        </div>
      )}
      {error && <p className={s.errorMsg}>{error}</p>}
      <button className={s.submitBtn} type="submit" disabled={submitting}>
        {submitting && <span className={s.spinner} />}
        {submitting ? 'Adding account…' : 'Add account'}
      </button>
    </form>
  );
}

// ── AccountSetup ──────────────────────────────────────────────────────────────

interface AccountSetupProps {
  onAccountAdded: () => void;
}

export function AccountSetup({ onAccountAdded }: AccountSetupProps) {
  const [gmailLoading, setGmailLoading] = useState(false);
  const [msLoading, setMsLoading] = useState(false);
  const [showManual, setShowManual] = useState(false);
  const [showMsAppPassword, setShowMsAppPassword] = useState(false);
  const [oauthError, setOauthError] = useState<string | null>(null);

  const startOAuth = async (path: string, done: (loading: boolean) => void) => {
    done(true);
    setOauthError(null);
    try {
      const data = await api.get<{ url?: string; error?: string }>(path, { silent: true });
      if (!data.url) {
        setOauthError(data.error ?? 'OAuth not available');
        done(false);
        return;
      }
      window.location.href = data.url;
    } catch (e) {
      setOauthError(e instanceof Error ? e.message : 'Network error');
      done(false);
    }
  };

  const handleConnectGmail = () => startOAuth('/api/v1/auth/gmail/authorize', setGmailLoading);
  const handleConnectMicrosoft = () => startOAuth('/api/v1/auth/microsoft/authorize', setMsLoading);

  return (
    <div className={styles.wrap}>
      <div className={styles.card}>
        <h1 className={styles.title}>email-rs</h1>
        <p className={styles.subtitle}>Connect your first account to get started</p>

        <button
          className={styles.gmailBtn}
          type="button"
          onClick={handleConnectGmail}
          disabled={gmailLoading || msLoading}
        >
          {gmailLoading ? <span className={styles.spinner} /> : <GoogleGIcon />}
          {gmailLoading ? 'Connecting…' : 'Connect Gmail'}
        </button>

        <button
          className={styles.msBtn}
          type="button"
          onClick={handleConnectMicrosoft}
          disabled={gmailLoading || msLoading || showMsAppPassword}
        >
          {msLoading ? <span className={styles.spinner} /> : <MicrosoftIcon />}
          {msLoading ? 'Connecting…' : 'Connect Microsoft 365'}
        </button>

        {!showMsAppPassword && (
          <button
            className={styles.manualLink}
            type="button"
            onClick={() => setShowMsAppPassword(true)}
          >
            Work account blocked? Use app password instead
          </button>
        )}

        {showMsAppPassword && (
          <MicrosoftAppPasswordForm onSuccess={onAccountAdded} styles={styles} />
        )}

        {oauthError && <p className={styles.errorMsg}>{oauthError}</p>}

        {!showManual && (
          <button
            className={styles.manualLink}
            type="button"
            onClick={() => setShowManual(true)}
          >
            Add manually (IMAP/SMTP)
          </button>
        )}

        {showManual && (
          <ManualForm onSuccess={onAccountAdded} styles={styles} />
        )}
      </div>
    </div>
  );
}
