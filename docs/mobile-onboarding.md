# Mobile onboarding and server switching

1. Enter the HTTPS origin of the email-rs server, without `/api/v1`.
2. Sign in with the application username and password. On a new loopback-hosted server, select first-run setup and choose a password of at least 12 characters.
3. The app validates `/api/v1/app-auth/me` before showing cached navigation. Expired or revoked sessions return to onboarding.
4. Use **Account → Log out or switch server** to revoke the server session, remove the Keychain/Keystore value, and return to server selection.

Remote plain HTTP origins are rejected. `http://localhost` and `http://127.0.0.1` are accepted only for local development. Native tokens are stored by `@aparajita/capacitor-secure-storage`; web preview sessions remain memory-only.
