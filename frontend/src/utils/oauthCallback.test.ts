import { describe, expect, it, vi } from 'vitest';

import { handleOAuthCallback } from './oauthCallback';

describe('handleOAuthCallback', () => {
  it('clears a successful OAuth callback from the address bar', () => {
    const replaceState = vi.fn();

    expect(handleOAuthCallback(
      { pathname: '/inbox', search: '?oauth=success' },
      { replaceState },
    )).toBeNull();
    expect(replaceState).toHaveBeenCalledWith(null, '', '/inbox');
  });

  it('returns a decoded OAuth error and clears it from the address bar', () => {
    const replaceState = vi.fn();

    expect(handleOAuthCallback(
      { pathname: '/', search: '?oauth=error&msg=access_denied%3Cscript%3E' },
      { replaceState },
    )).toBe('access_denied<script>');
    expect(replaceState).toHaveBeenCalledWith(null, '', '/');
  });

  it('leaves unrelated query strings unchanged', () => {
    const replaceState = vi.fn();

    expect(handleOAuthCallback(
      { pathname: '/inbox', search: '?view=unread' },
      { replaceState },
    )).toBeNull();
    expect(replaceState).not.toHaveBeenCalled();
  });
});
