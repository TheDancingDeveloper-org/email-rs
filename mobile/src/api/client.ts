import { Capacitor } from '@capacitor/core';
import { SecureStorage } from '@aparajita/capacitor-secure-storage';
import { assertContractRoute } from './openapi.generated';

const CONFIG_KEY = 'server-session';

export interface ServerSession {
  serverUrl: string;
  token: string;
  expiresAt: number;
}

let session: ServerSession | null = null;

export function normalizeServerUrl(value: string): string {
  const url = new URL(value.trim());
  const loopback = ['localhost', '127.0.0.1', '::1'].includes(url.hostname);
  if (url.protocol !== 'https:' && !(url.protocol === 'http:' && loopback)) {
    throw new Error('Use HTTPS for remote servers. HTTP is allowed only for loopback development.');
  }
  url.pathname = url.pathname.replace(/\/+$/, '');
  url.search = '';
  url.hash = '';
  return url.toString().replace(/\/$/, '');
}

export function configureSession(value: ServerSession | null): void {
  session = value;
}

export async function loadSession(): Promise<ServerSession | null> {
  if (!Capacitor.isNativePlatform()) return null;
  const value = await SecureStorage.getItem(CONFIG_KEY);
  if (!value) return null;
  let stored: ServerSession;
  try { stored = JSON.parse(value) as ServerSession; } catch { await clearSession(); return null; }
  if (!stored.serverUrl || !stored.token || stored.expiresAt <= Math.floor(Date.now() / 1000)) {
    await clearSession();
    return null;
  }
  session = stored;
  return stored;
}

export async function saveSession(value: ServerSession): Promise<void> {
  session = value;
  if (Capacitor.isNativePlatform()) await SecureStorage.setItem(CONFIG_KEY, JSON.stringify(value));
}

export async function clearSession(): Promise<void> {
  session = null;
  if (Capacitor.isNativePlatform()) await SecureStorage.remove(CONFIG_KEY);
}

function baseUrl(): string {
  return session ? `${session.serverUrl}/api/v1` : '/api/v1';
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  assertContractRoute(init?.method || 'GET', path);
  const headers = new Headers(init?.headers);
  if (session?.token) headers.set('Authorization', `Bearer ${session.token}`);
  const response = await fetch(`${baseUrl()}${path}`, { ...init, headers });
  const text = await response.text();
  let body: unknown = null;
  if (text) {
    try { body = JSON.parse(text); } catch { body = text; }
  }
  if (!response.ok) {
    const message = typeof body === 'object' && body && 'error' in body
      ? String((body as { error: unknown }).error)
      : typeof body === 'string' && body
        ? body
        : `${response.status} ${response.statusText}`;
    throw new Error(message);
  }
  return body as T;
}

async function requestBlob(path: string): Promise<Blob> {
  assertContractRoute('GET', path);
  const headers = new Headers();
  if (session?.token) headers.set('Authorization', `Bearer ${session.token}`);
  const response = await fetch(`${baseUrl()}${path}`, { headers });
  if (!response.ok) {
    const body = await response.text();
    throw new Error(body || `${response.status} ${response.statusText}`);
  }
  return response.blob();
}

export interface AuthSessionResponse {
  userId: string;
  sessionId: string;
  expiresAt: number;
  token: string;
}

export async function authenticate(
  serverUrl: string,
  username: string,
  password: string,
  bootstrap = false,
): Promise<ServerSession> {
  const normalized = normalizeServerUrl(serverUrl);
  const response = await fetch(`${normalized}/api/v1/app-auth/${bootstrap ? 'bootstrap' : 'login'}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password, client: 'bearer' }),
  });
  const body = await response.json().catch(() => ({})) as Partial<AuthSessionResponse> & { error?: string };
  if (!response.ok) throw new Error(body.error || `${response.status} ${response.statusText}`);
  if (!body.token || !body.expiresAt) throw new Error('Server did not return a bearer session');
  const value = { serverUrl: normalized, token: body.token, expiresAt: body.expiresAt };
  await saveSession(value);
  return value;
}

export async function testConnection(value: ServerSession): Promise<void> {
  configureSession(value);
  await request('/app-auth/me');
}

export async function logout(): Promise<void> {
  try { await json('POST', '/app-auth/logout'); } finally { await clearSession(); }
}

function json<T>(method: string, path: string, body?: unknown): Promise<T> {
  return request<T>(path, {
    method,
    headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

const segment = (value: string) => encodeURIComponent(value);

export interface MobileSendRequest {
  accountId: string;
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  textBody: string | null;
  htmlBody: string | null;
  inReplyTo: string | null;
  attachments: {
    filename: string;
    contentType: string;
    contentBase64: string;
  }[];
}

export interface MobileDraftResponse {
  id: string;
  occurrenceId: string;
  logicalId: string;
  messageId: string;
  status: string;
}

export const api = {
  accounts:        () => request('/accounts'),
  folders:         (accountId: string) => request(`/accounts/${segment(accountId)}/folders`),
  messages:        (folderId: string, page = 1) => request(`/folders/${segment(folderId)}/messages?page=${page}&per_page=40`),
  message:         (id: string) => request(`/messages/${segment(id)}`),
  attachment:      (messageId: string, attachmentId: string) => requestBlob(`/messages/${segment(messageId)}/attachments/${segment(attachmentId)}`),
  search:          (q: string) => request(`/search?q=${encodeURIComponent(q)}&limit=40`),
  markRead:        (id: string) => json('PATCH', `/messages/${segment(id)}`, { isRead: true }),
  flag:            (id: string, val: boolean) => json('PATCH', `/messages/${segment(id)}`, { isFlagged: val }),
  archive:         (id: string) => json('POST', `/messages/${segment(id)}/archive`),
  trash:           (id: string) => json('DELETE', `/messages/${segment(id)}`),
  send:            (body: MobileSendRequest) => json('POST', '/messages', body),
  createDraft:     (body: MobileSendRequest) => json<MobileDraftResponse>('POST', '/drafts', body),
  updateDraft:     (id: string, body: MobileSendRequest) => json<MobileDraftResponse>('PUT', `/drafts/${segment(id)}`, body),
  deleteDraft:     (id: string) => json('DELETE', `/drafts/${segment(id)}`),
  accountSettings: (accountId: string) => request(`/accounts/${segment(accountId)}/settings`),
  updateAccount:   (accountId: string, body: { syncDaysLimit?: number | null }) => json('PATCH', `/accounts/${segment(accountId)}`, body),
  updateFolder:    (folderId: string, body: { isExcluded?: boolean }) => json('PATCH', `/folders/${segment(folderId)}`, body),
  contactSuggest:  (q: string) => request<{ name: string | null; email: string }[]>(`/contacts/suggest?q=${encodeURIComponent(q)}`),
};
