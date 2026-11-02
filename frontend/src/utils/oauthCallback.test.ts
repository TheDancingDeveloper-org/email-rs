import { describe, expect, it, vi } from 'vitest';

import { handleOAuthCallback } from './oauthCallback';

describe('handleOAuthCallback', () => {
  it('clears a successful OAuth callback from the address bar', () => {
    const replaceState = vi.fn();

    expect(handleOAuthCallback(
      { pathname: '/inbox', search: '?oauth=success' },
      { replaceState },
    )).toBe(true);
    expect(replaceState).toHaveBeenCalledWith(null, '', '/inbox');
  });

  it('leaves unrelated query strings unchanged', () => {
    const replaceState = vi.fn();

    expect(handleOAuthCallback(
      { pathname: '/inbox', search: '?view=unread' },
      { replaceState },
    )).toBe(false);
    expect(replaceState).not.toHaveBeenCalled();
  });
});
