# FluentGUI Phase 0 Fixtures

These SQLite databases are generated locally and ignored by git. The source of
truth is the deterministic generator:

```bash
cargo run -p email-server --bin seed_fixtures -- fixtures/fluentgui-phase0
```

It writes:

- `no-accounts.sqlite`: migrated schema with no account rows, for first-run setup.
- `one-account.sqlite`: one Gmail-shaped offline account with folders, labels,
  mail states, calendar fixtures, rules, and webhooks.
- `two-accounts.sqlite`: the one-account dataset plus a Microsoft-shaped offline
  account for multi-account navigation and settings checks.

Assumption: fixture accounts use password/basic or app-password auth with
non-routable `example.test` hosts so local verification does not require OAuth
credentials or contact real mail providers. Real Gmail/Microsoft OAuth remains
covered by the manual service checks in the migration plan.

Example local run against a fixture:

```bash
DATABASE_URL=sqlite://$PWD/fixtures/fluentgui-phase0/one-account.sqlite \
EMAIL_RS_HEADLESS=1 \
cargo run -p email-server
```

Open `http://127.0.0.1:8585` after the server starts.
