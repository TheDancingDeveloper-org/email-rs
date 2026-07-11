import { afterEach, describe, expect, it, vi } from 'vitest';
import { api, configureSession, normalizeServerUrl } from './client';

afterEach(() => {
  configureSession(null);
  vi.unstubAllGlobals();
});

describe('mobile server contract', () => {
  it('requires HTTPS except for loopback development', () => {
    expect(normalizeServerUrl('https://mail.example.test/')).toBe('https://mail.example.test');
    expect(normalizeServerUrl('http://localhost:8585/')).toBe('http://localhost:8585');
    expect(() => normalizeServerUrl('http://mail.example.test')).toThrow(/HTTPS/);
  });

  it('sends bearer credentials and correct mutation verbs', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true, status: 200, statusText: 'OK', text: async () => '{}',
    });
    vi.stubGlobal('fetch', fetchMock);
    configureSession({ serverUrl: 'https://mail.example.test', token: 'secret-token', expiresAt: 99 });

    await api.archive('m 1');
    await api.trash('m2');

    expect(fetchMock.mock.calls[0][0]).toBe('https://mail.example.test/api/v1/messages/m%201/archive');
    expect(fetchMock.mock.calls[0][1].method).toBe('POST');
    expect((fetchMock.mock.calls[0][1].headers as Headers).get('Authorization')).toBe('Bearer secret-token');
    expect(fetchMock.mock.calls[1][1].method).toBe('DELETE');
  });

  it('surfaces structured server failures', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: false, status: 409, statusText: 'Conflict', text: async () => '{"error":"remote mutation failed"}',
    }));
    await expect(api.trash('m1')).rejects.toThrow('remote mutation failed');
  });
});
