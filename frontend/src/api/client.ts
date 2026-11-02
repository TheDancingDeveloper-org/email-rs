// The single web API client. All /api/v1 requests go through request() so that
// failures are raised as ApiError and (for writes) reported to the user.
import type { Account, Message } from '../types';

export class ApiError extends Error {
  readonly status: number;
  constructor(status: number, message: string) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
  }
}

type ErrorListener = (error: ApiError | Error) => void;
const listeners = new Set<ErrorListener>();

/** Subscribe to failed write requests (for a toast/banner). Returns an unsubscribe. */
export function onApiError(listener: ErrorListener): () => void {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}

export function reportApiError(error: unknown): void {
  const err = error instanceof Error ? error : new Error(String(error));
  if (err.name === 'AbortError') return;
  listeners.forEach((l) => l(err));
}

export interface RequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  body?: unknown;
  signal?: AbortSignal;
  /** Do not notify onApiError listeners; the caller shows the error itself. */
  silent?: boolean;
}

async function errorFromResponse(res: Response): Promise<ApiError> {
  let message = '';
  try {
    const body = (await res.json()) as { error?: unknown };
    if (typeof body.error === 'string') message = body.error;
  } catch {
    // non-JSON error body
  }
  return new ApiError(res.status, message || `HTTP ${res.status}`);
}

export async function request<T = void>(path: string, opts: RequestOptions = {}): Promise<T> {
  const method = opts.method ?? 'GET';
  const init: RequestInit = { method, signal: opts.signal };
  if (opts.body !== undefined) {
    init.headers = { 'Content-Type': 'application/json' };
    init.body = JSON.stringify(opts.body);
  }
  try {
    const res = await fetch(path, init);
    if (!res.ok) throw await errorFromResponse(res);
    if (res.status === 204) return undefined as T;
    const text = await res.text();
    return (text ? JSON.parse(text) : undefined) as T;
  } catch (e) {
    if (!opts.silent && method !== 'GET') reportApiError(e);
    throw e;
  }
}

export const api = {
  get: <T>(path: string, opts?: RequestOptions) => request<T>(path, opts),
  post: <T = void>(path: string, body?: unknown, opts?: RequestOptions) =>
    request<T>(path, { ...opts, method: 'POST', body }),
  put: <T = void>(path: string, body?: unknown, opts?: RequestOptions) =>
    request<T>(path, { ...opts, method: 'PUT', body }),
  patch: <T = void>(path: string, body?: unknown, opts?: RequestOptions) =>
    request<T>(path, { ...opts, method: 'PATCH', body }),
  delete: <T = void>(path: string, opts?: RequestOptions) =>
    request<T>(path, { ...opts, method: 'DELETE' }),
};

// ---- Typed endpoints ----

export interface CursorMessages {
  messages: Message[];
  nextCursor: string | null;
}

/** Cursor-paginated folder messages (#58). Pass the previous nextCursor, or '' for the first page. */
export async function listFolderMessagesCursor(
  folderId: string,
  cursor = '',
  perPage = 50,
  signal?: AbortSignal,
): Promise<CursorMessages> {
  const q = new URLSearchParams({ cursor, per_page: String(perPage) });
  return api.get<CursorMessages>(`/api/v1/folders/${encodeURIComponent(folderId)}/messages?${q}`, { signal });
}

export const listAccounts = (signal?: AbortSignal) => api.get<Account[]>('/api/v1/accounts', { signal });

export const archiveMessage = (id: string) => api.post(`/api/v1/messages/${id}/archive`);
export const trashMessage = (id: string) => api.delete(`/api/v1/messages/${id}`);
export const patchMessage = (id: string, patch: Record<string, unknown>) =>
  api.patch(`/api/v1/messages/${id}`, patch);
export const markFolderRead = (folderId: string) => api.post(`/api/v1/folders/${folderId}/mark-read`);
