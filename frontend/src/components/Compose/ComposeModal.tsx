import { useState, useCallback, useEffect } from 'react';
import styles from './ComposeModal.module.css';
import { RecipientInput } from './RecipientInput';
import { useAppStore } from '../../store';
import type { AccountSettings } from '../../types';

export function ComposeModal() {
  const { compose, closeCompose } = useAppStore();

  const [to, setTo] = useState('');
  const [cc, setCc] = useState('');
  const [bcc, setBcc] = useState('');
  const [subject, setSubject] = useState('');
  const [body, setBody] = useState('');
  const [signature, setSignature] = useState('');
  const [showCc, setShowCc] = useState(false);
  const [showBcc, setShowBcc] = useState(false);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [attachments, setAttachments] = useState<File[]>([]);

  useEffect(() => {
    if (compose) {
      setTo(compose.to);
      setCc('');
      setBcc('');
      setShowCc(false);
      setShowBcc(false);
      setSubject(compose.subject);
      setBody(compose.mode === 'forward' ? buildForwardBody(compose.quotedText, compose.quotedFrom) : '');
      setError(null);
      setSending(false);
      setAttachments([]);
      // Fetch account signature.
      fetch(`/api/v1/accounts/${compose.accountId}/settings`)
        .then((r) => r.json())
        .then((s: AccountSettings) => { if (s.signature) setSignature(s.signature); })
        .catch(() => {});
    }
  }, [compose]);

  const splitAddrs = (s: string) => s.split(',').map((x) => x.trim()).filter(Boolean);

  const handleSend = useCallback(async () => {
    if (!compose || !to.trim() || !body.trim()) return;
    setSending(true);
    setError(null);
    try {
      const sigBlock = signature
        ? ['', '', `--${String.fromCharCode(32)}`, signature].join('\n')
        : '';
      const fullBody = (compose.mode === 'reply' && compose.quotedText
        ? body + '\n\n' + buildQuotedBlock(compose.quotedText, compose.quotedFrom)
        : body) + sigBlock;
      const encodedAttachments = await Promise.all(attachments.map(async (file) => ({
        filename: file.name,
        contentType: file.type || 'application/octet-stream',
        contentBase64: bytesToBase64(new Uint8Array(await file.arrayBuffer())),
      })));
      const resp = await fetch('/api/v1/messages', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          accountId: compose.accountId,
          to: splitAddrs(to),
          cc: splitAddrs(cc),
          bcc: splitAddrs(bcc),
          subject,
          textBody: fullBody,
          inReplyTo: compose.inReplyTo,
          attachments: encodedAttachments,
        }),
      });
      if (!resp.ok) {
        const err = await resp.json().catch(() => ({ error: 'Send failed' }));
        setError((err as { error?: string }).error ?? 'Send failed');
      } else {
        closeCompose();
      }
    } catch {
      setError('Network error');
    } finally {
      setSending(false);
    }
  }, [attachments, bcc, body, cc, closeCompose, compose, signature, subject, to]);

  if (!compose) return null;

  const modeLabel = compose.mode === 'reply' ? 'Reply' : compose.mode === 'forward' ? 'Forward' : 'New message';

  return (
    <div className={styles.pane}>
      <div className={styles.header}>
        <span className={styles.modeTag} data-mode={compose.mode}>{modeLabel}</span>
        <span className={styles.headerSubject}>{compose.subject || 'No subject'}</span>
        <button className={styles.closeBtn} type="button" onClick={closeCompose} aria-label="Discard">
          ×
        </button>
      </div>

      <div className={styles.fields}>
        <div className={styles.fieldRow}>
          <span className={styles.fieldLabel}>To</span>
          <RecipientInput
            value={to}
            onChange={setTo}
            placeholder="recipient@example.com"
            autoFocus={!to}
          />
          <div className={styles.fieldActions}>
            {!showCc && <button type="button" className={styles.fieldToggle} onClick={() => setShowCc(true)}>Cc</button>}
            {!showBcc && <button type="button" className={styles.fieldToggle} onClick={() => setShowBcc(true)}>Bcc</button>}
          </div>
        </div>
        {showCc && (
          <div className={styles.fieldRow}>
            <span className={styles.fieldLabel}>Cc</span>
            <RecipientInput value={cc} onChange={setCc} placeholder="cc@example.com" autoFocus />
          </div>
        )}
        {showBcc && (
          <div className={styles.fieldRow}>
            <span className={styles.fieldLabel}>Bcc</span>
            <RecipientInput value={bcc} onChange={setBcc} placeholder="bcc@example.com" autoFocus={!showCc} />
          </div>
        )}
        <div className={styles.fieldRow}>
          <span className={styles.fieldLabel}>Subject</span>
          <input
            className={styles.fieldInput}
            type="text"
            value={subject}
            onChange={(e) => setSubject(e.target.value)}
            placeholder="Subject"
          />
        </div>
      </div>

      <div className={styles.body}>
        <textarea
          className={styles.bodyArea}
          value={body}
          onChange={(e) => setBody(e.target.value)}
          placeholder="Write your message…"
          autoFocus={!!to}
        />

        {signature && (
          <div className={styles.signatureBlock}>
            <div className={styles.signatureDivider}>--</div>
            <pre className={styles.signatureText}>{signature}</pre>
          </div>
        )}

        {compose.mode === 'reply' && compose.quotedText && (
          <div className={styles.quotedBlock}>
            <div className={styles.quotedFrom}>
              {compose.quotedFrom ? `— ${compose.quotedFrom} wrote:` : '— Original message:'}
            </div>
            <pre className={styles.quotedText}>{compose.quotedText}</pre>
          </div>
        )}
      </div>

      <div className={styles.footer}>
        <button
          className={styles.sendBtn}
          type="button"
          onClick={handleSend}
          disabled={sending || !to.trim() || !body.trim()}
        >
          {sending ? 'Sending…' : 'Send'}
        </button>
        <button className={styles.discardBtn} type="button" onClick={closeCompose}>
          Discard
        </button>
        <label className={styles.discardBtn}>
          Attach
          <input
            type="file"
            multiple
            hidden
            onChange={(event) => {
              const selected = Array.from(event.target.files ?? []);
              setAttachments((current) => [...current, ...selected]);
              event.target.value = '';
            }}
          />
        </label>
        {attachments.map((file, index) => (
          <button
            key={`${file.name}-${file.size}-${index}`}
            className={styles.discardBtn}
            type="button"
            title="Remove attachment"
            onClick={() => setAttachments((current) => current.filter((_, itemIndex) => itemIndex !== index))}
          >
            {file.name} ×
          </button>
        ))}
        {error && <span className={styles.errorMsg}>{error}</span>}
      </div>
    </div>
  );
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunkSize = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
  }
  return btoa(binary);
}

function buildQuotedBlock(text: string, from?: string): string {
  const prefix = from ? `${from} wrote:\n` : 'Original message:\n';
  return prefix + text.split('\n').map((l) => `> ${l}`).join('\n');
}

function buildForwardBody(text?: string, from?: string): string {
  if (!text) return '';
  const header = from ? `---------- Forwarded message from ${from} ----------\n\n` : '---------- Forwarded message ----------\n\n';
  return header + text;
}
