import { useState, useRef, useCallback } from 'react';
import styles from './ComposeScreen.module.css';
import { useAppStore } from '../store';
import { api } from '../api/client';

interface Contact { name: string | null; email: string; }

export function ComposeScreen() {
  const { closeCompose, accounts } = useAppStore();
  const [accountId, setAccountId] = useState(() => accounts[0]?.id ?? '');
  const [to, setTo] = useState('');
  const [subject, setSubject] = useState('');
  const [body, setBody] = useState('');
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);

  const [suggestions, setSuggestions] = useState<Contact[]>([]);
  const [showSuggestions, setShowSuggestions] = useState(false);
  const suggestTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleToChange = useCallback((val: string) => {
    setTo(val);
    // Autocomplete on the last address token (after last comma/space)
    const token = val.split(/[,;]\s*/).pop()?.trim() ?? '';
    if (suggestTimer.current) clearTimeout(suggestTimer.current);
    if (token.length < 2) { setSuggestions([]); setShowSuggestions(false); return; }
    suggestTimer.current = setTimeout(async () => {
      try {
        const rows = await api.contactSuggest(token);
        setSuggestions(rows);
        setShowSuggestions(rows.length > 0);
      } catch { setSuggestions([]); setShowSuggestions(false); }
    }, 200);
  }, []);

  const pickSuggestion = (c: Contact) => {
    // Replace the last token with the chosen email
    const parts = to.split(/([,;]\s*)/);
    parts[parts.length - 1] = c.email + ', ';
    setTo(parts.join(''));
    setSuggestions([]);
    setShowSuggestions(false);
  };

  const handleSend = async () => {
    if (!accountId || !to.trim() || sending) return;
    setSending(true);
    setSendError(null);
    try {
      await api.send({
        accountId,
        to: to.split(/[,;]/).map((value) => value.trim()).filter(Boolean),
        cc: [],
        bcc: [],
        subject,
        textBody: body || null,
        htmlBody: null,
        inReplyTo: null,
      });
      closeCompose();
    } catch (cause) {
      setSendError(cause instanceof Error ? cause.message : 'Unable to send message');
    } finally {
      setSending(false);
    }
  };

  return (
    <div className={styles.screen}>
      <header className={styles.header}>
        <button className={styles.cancel} onClick={closeCompose}>Cancel</button>
        <span className={styles.title}>New Message</span>
        <button
          className={`${styles.send} ${(!to.trim() || sending) ? styles.sendDisabled : ''}`}
          onClick={handleSend}
          disabled={!to.trim() || sending}
        >
          {sending ? 'Sending…' : 'Send'}
        </button>
      </header>

      <div className={styles.fields}>
        <div className={styles.field}>
          <label className={styles.fieldLabel}>From</label>
          <select className={styles.fieldInput} value={accountId} onChange={(event) => setAccountId(event.target.value)}>
            {accounts.map((account) => <option key={account.id} value={account.id}>{account.name} · {account.email}</option>)}
          </select>
        </div>
        <div className={styles.divider} />
        <div className={styles.toFieldWrap}>
          <div className={styles.field}>
            <label className={styles.fieldLabel}>To</label>
            <input
              className={styles.fieldInput}
              type="email"
              value={to}
              onChange={(e) => handleToChange(e.target.value)}
              onBlur={() => setTimeout(() => setShowSuggestions(false), 150)}
              onFocus={() => { if (suggestions.length > 0) setShowSuggestions(true); }}
              placeholder="recipient@example.com"
              autoCapitalize="none"
              autoCorrect="off"
            />
          </div>
          {showSuggestions && (
            <ul className={styles.suggestions}>
              {suggestions.map((c) => (
                <li key={c.email}>
                  <button className={styles.suggestionBtn} onMouseDown={() => pickSuggestion(c)}>
                    {c.name && <span className={styles.suggName}>{c.name}</span>}
                    <span className={styles.suggEmail}>{c.email}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className={styles.divider} />
        <div className={styles.field}>
          <label className={styles.fieldLabel}>Subject</label>
          <input
            className={styles.fieldInput}
            type="text"
            value={subject}
            onChange={(e) => setSubject(e.target.value)}
            placeholder="Subject"
          />
        </div>
        <div className={styles.divider} />
        <textarea
          className={styles.bodyInput}
          value={body}
          onChange={(e) => setBody(e.target.value)}
          placeholder="Write your message…"
        />
        {sendError && <div role="alert" style={{ padding: '10px 16px', color: 'var(--danger)', fontSize: 13 }}>{sendError}</div>}
      </div>
    </div>
  );
}
