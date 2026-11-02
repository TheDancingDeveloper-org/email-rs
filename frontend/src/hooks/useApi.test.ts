import { afterEach, describe, expect, it, vi } from 'vitest';

import { apiPost } from './useApi';

describe('apiPost', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('uses the API error message for failed responses', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(
      JSON.stringify({ error: 'Account already exists' }),
      { status: 409, headers: { 'Content-Type': 'application/json' } },
    )));

    await expect(apiPost('/api/v1/accounts', {})).rejects.toThrow('Account already exists');
  });

  it('falls back to the HTTP status for non-JSON errors', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('bad gateway', { status: 502 })));

    await expect(apiPost('/api/v1/accounts', {})).rejects.toThrow('HTTP 502');
  });
});
