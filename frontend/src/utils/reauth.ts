import type { Account } from '../types';

/** SSE `sync` event types that change an account's needs-reauth state. */
export const ACCOUNT_STATUS_EVENTS = ['account_needs_reauth', 'account_reauthed'] as const;

export function isAccountStatusEvent(type: string): boolean {
  return (ACCOUNT_STATUS_EVENTS as readonly string[]).includes(type);
}

/**
 * The authorize endpoint that re-consents an existing OAuth account. Passing
 * `accountId` makes the server refresh that account's grant (and refuse a
 * different mailbox) instead of connecting a new one. Null for accounts that
 * are not OAuth-backed.
 */
export function reauthorizePath(account: Pick<Account, 'id' | 'providerType'>): string | null {
  const provider =
    account.providerType === 'gmail'
      ? 'gmail'
      : account.providerType === 'microsoft365'
        ? 'microsoft'
        : null;
  if (!provider) return null;
  return `/api/v1/auth/${provider}/authorize?accountId=${encodeURIComponent(account.id)}`;
}
