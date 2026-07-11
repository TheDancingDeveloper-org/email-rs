import { FormEvent, useState } from 'react';
import { authenticate } from '../api/client';

interface Props { onAuthenticated: () => void }

export function ServerLoginScreen({ onAuthenticated }: Props) {
  const [serverUrl, setServerUrl] = useState('https://');
  const [username, setUsername] = useState('admin');
  const [password, setPassword] = useState('');
  const [setup, setSetup] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true); setError(null);
    try {
      await authenticate(serverUrl, username, password, setup);
      onAuthenticated();
    } catch (value) {
      setError(value instanceof Error ? value.message : String(value));
    } finally { setBusy(false); }
  }

  return <main style={{ padding: 24, maxWidth: 480, margin: '48px auto' }}>
    <h1>{setup ? 'Set up rsMail' : 'Connect to rsMail'}</h1>
    <p>Connect to your self-hosted server. Remote servers must use HTTPS.</p>
    <form onSubmit={submit} style={{ display: 'grid', gap: 12 }}>
      <label>Server URL<input aria-label="Server URL" value={serverUrl} onChange={e => setServerUrl(e.target.value)} required /></label>
      <label>Username<input aria-label="Username" value={username} onChange={e => setUsername(e.target.value)} required /></label>
      <label>Password<input aria-label="Password" type="password" value={password} onChange={e => setPassword(e.target.value)} minLength={setup ? 12 : undefined} required /></label>
      {error && <p role="alert" style={{ color: 'var(--danger, #b42318)' }}>{error}</p>}
      <button disabled={busy}>{busy ? 'Connecting…' : setup ? 'Create user' : 'Log in'}</button>
      <button type="button" disabled={busy} onClick={() => { setSetup(!setup); setError(null); }}>
        {setup ? 'I already have a user' : 'First-run server setup'}
      </button>
    </form>
  </main>;
}
