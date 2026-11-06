import { describe, expect, it } from 'vitest';

import { isAccountStatusEvent, reauthorizePath } from './reauth';

describe('reauthorizePath', () => {
  it('re-consents Gmail and Microsoft accounts by id', () => {
    expect(reauthorizePath({ id: 'a b', providerType: 'gmail' })).toBe(
      '/api/v1/auth/gmail/authorize?accountId=a%20b',
    );
    expect(reauthorizePath({ id: 'm1', providerType: 'microsoft365' })).toBe(
      '/api/v1/auth/microsoft/authorize?accountId=m1',
    );
  });

  it('has no OAuth flow for password accounts', () => {
    expect(reauthorizePath({ id: 'i1', providerType: 'imap' })).toBeNull();
  });
});

describe('isAccountStatusEvent', () => {
  it('recognises the needs-reauth lifecycle events only', () => {
    expect(isAccountStatusEvent('account_needs_reauth')).toBe(true);
    expect(isAccountStatusEvent('account_reauthed')).toBe(true);
    expect(isAccountStatusEvent('sync_complete')).toBe(false);
  });
});
