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
): boolean {
  const params = new URLSearchParams(location.search);
  if (params.get('oauth') !== 'success') return false;

  history.replaceState(null, '', location.pathname);
  return true;
}
