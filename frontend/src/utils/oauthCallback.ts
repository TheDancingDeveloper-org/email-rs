interface OAuthCallbackLocation {
  pathname: string;
  search: string;
}

interface OAuthCallbackHistory {
  replaceState(data: unknown, unused: string, url?: string | URL | null): void;
}

export function handleOAuthCallback(
  location: OAuthCallbackLocation,
  history: OAuthCallbackHistory,
): string | null {
  const params = new URLSearchParams(location.search);
  const oauthStatus = params.get('oauth');
  if (oauthStatus !== 'success' && oauthStatus !== 'error') return null;

  history.replaceState(null, '', location.pathname);
  return oauthStatus === 'error'
    ? params.get('msg') || 'OAuth sign-in failed'
    : null;
}
