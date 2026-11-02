import { afterEach, describe, expect, it, vi } from 'vitest';

import { ApiError, api, listFolderMessagesCursor, onApiError, request } from './client';

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

describe('api client', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('throws ApiError with the server message and status', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(json({ error: 'nope' }, 409)));
    const err = await request('/api/v1/x').catch((e) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(409);
    expect(err.message).toBe('nope');
  });

  it('falls back to HTTP status for non-JSON errors', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('bad gateway', { status: 502 })));
    await expect(api.get('/api/v1/x')).rejects.toThrow('HTTP 502');
  });

  it('propagates network failures', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')));
    await expect(api.get('/api/v1/x')).rejects.toThrow('Failed to fetch');
  });

  it('reports failed writes to listeners but not reads or silent writes', async () => {
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => json({ error: 'bad' }, 500)));
    const seen: string[] = [];
    const off = onApiError((e) => seen.push(e.message));
    await api.get('/api/v1/x').catch(() => undefined);
    await api.post('/api/v1/x', {}, { silent: true }).catch(() => undefined);
    expect(seen).toEqual([]);
    await api.post('/api/v1/x', {}).catch(() => undefined);
    await api.delete('/api/v1/x').catch(() => undefined);
    expect(seen).toEqual(['bad', 'bad']);
    off();
  });

  it('sends JSON bodies and parses empty/204 responses', async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
    vi.stubGlobal('fetch', fetchMock);
    await expect(api.patch('/api/v1/x', { a: 1 })).resolves.toBeUndefined();
    const init = fetchMock.mock.calls[0][1] as RequestInit;
    expect(init.method).toBe('PATCH');
    expect(init.body).toBe('{"a":1}');
  });

  it('fetches cursor pages', async () => {
    const fetchMock = vi.fn().mockResolvedValue(json({ messages: [], nextCursor: 'abc' }));
    vi.stubGlobal('fetch', fetchMock);
    const page = await listFolderMessagesCursor('f1', '', 25);
    expect(page.nextCursor).toBe('abc');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/v1/folders/f1/messages?cursor=&per_page=25');
  });
});
