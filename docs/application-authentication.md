# Application authentication

Provider credentials authorize email-rs to access Gmail, Microsoft, or IMAP. Application authentication is separate: it protects the email-rs API, cached mail, account settings, and provider authorization flows from unauthenticated clients.

## Model

- One application user is bootstrapped explicitly.
- Passwords are stored as Argon2id hashes in `app_users`.
- Browser and cookie-capable desktop sessions receive an HTTP-only, SameSite=Strict cookie named `email_rs_session`.
- Native mobile clients request bearer credentials.
- Only SHA-256 token hashes are stored in `app_sessions`.
- Cookie sessions default to 12 hours; bearer sessions default to 30 days.
- Refresh rotates the credential. Logout and revocation delete it server-side.
- OAuth authorization state contains the initiating application user/session and optional account ID. A revoked or expired initiating session invalidates the callback state.

## Public endpoints

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/v1/health` | Minimal health response (`ok`). |
| `GET` | `/api/v1/openapi.json` | Generated route/security contract. |
| `POST` | `/api/v1/app-auth/bootstrap` | Create the first user from a direct loopback connection only. |
| `POST` | `/api/v1/app-auth/login` | Verify the password and issue a cookie or bearer session. |
| `GET` | `/api/v1/auth/gmail/callback` | Google OAuth callback. |
| `GET` | `/api/v1/auth/microsoft/callback` | Microsoft OAuth callback. |

All other API routes, including OAuth authorization-start routes and SSE events, require a valid application session.

Bootstrap and login accept camelCase JSON:

```json
{
  "username": "admin",
  "password": "a long deployment secret",
  "client": "cookie"
}
```

Use `"client": "bearer"` for a native mobile login. A successful response contains `userId`, `sessionId`, and `expiresAt`. Bearer responses additionally contain `token`; cookie responses set the credential only in `Set-Cookie` and do not expose it in JSON.

## Protected session endpoints

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/v1/app-auth/me` | Return the authenticated user, session, and client kind. |
| `POST` | `/api/v1/app-auth/refresh` | Rotate the current credential and extend expiry. |
| `POST` | `/api/v1/app-auth/logout` | Revoke the current session and expire its cookie. |
| `DELETE` | `/api/v1/app-auth/sessions/{id}` | Revoke a session belonging to the current user. |

Send bearer credentials as `Authorization: Bearer <token>`. Browser fetches use the same-origin cookie automatically.

## Origin and CORS rules

Cookie-authenticated POST, PUT, PATCH, and DELETE requests require an `Origin` matching the request host, `EMAIL_RS_PUBLIC_URL`, or `EMAIL_RS_ALLOWED_ORIGINS`. Bearer mutations do not require Origin, but CORS still controls whether a browser origin can read the response. Same-origin web access needs no CORS header.

## Native clients

- Fluent desktop provides bearer login and loopback first-run setup, stores the token in a user-only file, shares it across API/SSE/download clients, and revokes it on logout.
- Mobile provides first-run server selection, rejects non-loopback HTTP URLs, validates the session before opening mail, and stores bearer sessions in iOS Keychain or Android Keystore-backed encrypted storage. Logout returns to onboarding so another server can be selected.
- Web builds of the mobile project deliberately do not persist bearer credentials because browser storage is not a secure secret store.

Authentication and updated native clients must ship together. The canonical protected-route inventory is exercised anonymously in server tests, and `/api/v1/openapi.json` exposes the same inventory to client tooling.
