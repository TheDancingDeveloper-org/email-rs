# email-rs and myAIAgent integration review

## Current implementation state (authoritative handoff — 2026-07-12 continuation)

**Read this section first.** It supersedes the older 2026-07-12 handoff and
the original review below. The continuation work is captured in the two commits
identified here; do not restart from an older handoff.

The answer to **"has all this work been completed?" remains no**. The core
service boundary, v0.5 release, legacy-ingestion retirement, webhook indexing,
and most client-completeness work are done. Provider/platform validation and
synchronized draft CRUD are still open. The post-v0.5 work described below
also needs a new release boundary; do not amend or silently republish v0.5.0.

### Repository state at this handoff

`email-rs` (`/home/sprooty/Working/Active/apps/email-rs`):

- `main`/`origin/main` are at `9d804c3` (`v0.5.0`). The v0.5 release contains
  durable secure events, credential encryption, stable logical IDs, generated
  SDK publication, and the release/CI repairs.
- The commit containing this handoff is the post-v0.5 client-completeness
  slice. It adds SMTP/MIME outgoing attachments (including CC/BCC), attachment
  validation/size limits, Gmail `X-GM-THRID` identity, RFC `In-Reply-To`
  reconciliation for generic IMAP, and web/mobile folder, label, and smart-view
  pagination.
- Standalone web and mobile compose now select and send attachments. Their
  reading panes now list attachment filenames/sizes and expose authenticated
  downloads. The web static bundle has been rebuilt, so the old hashed JS/CSS
  assets are replaced by the newly generated hashed assets in the same commit.
- `clients/python` has been regenerated from the current OpenAPI and now
  includes `OutgoingAttachment`/`SendRequest.attachments`. It still reports
  `0.5.0`; before publishing this post-v0.5 contract, choose and apply the next
  synchronized version across Rust/OpenAPI/Python, then publish immutably.

`myAIAgent` (`/home/sprooty/Working/Active/myAIAgent`):

- Commit `30db0ff` migrates the Android mail client to the gateway. Its parent
  `c66354f` was the previous `main`/`origin/main`. Commit `6a567c7` completed webhook
  indexing and removed legacy IMAP/Gmail-mail ingestion while preserving the
  independent Google Calendar connector. Compose pins email-rs v0.5.0 by digest
  and the backend consumes published `email-rs-client==0.5.0`.
- `ApiService`, `EmailViewModel`, and `EmailScreen` now use `/api/mail` for
  status, account/folder discovery, paginated inbox metadata, full-message
  loading, triage, AI drafting, explicit send, and indexed-email deep links.
- The backend adds `GET /api/mail/indexed/{email_id}`, resolving the persisted
  AI/search row through `EmailRsMessageLink.logical_id` to the current
  occurrence ID. Tests and generated API docs cover this route.
- No database migration is required for this Android slice; it uses the mapping
  tables already landed in migration `020_email_rs_webhook_index.py`.

### Recommendation status based on the actual branches/worktrees

| # | Recommendation | Current state | Remaining evidence or work |
|---|---|---|---|
| 1 | Machine tokens, complete OpenAPI, logical IDs, synchronized versioning | **Complete for v0.5; next version pending for dirty worktree** | v0.5 is tagged/released. Apply a new synchronized version before publishing the attachment/threading contract. |
| 2 | Generated Python client and cross-repository acceptance | **Mostly complete** | v0.5 SDK is published/consumed and the real acceptance script covers webhook retry, logical reconciliation, search, and persisted triage. It is not a required CI job. |
| 3 | Immutable sidecar pin in compose and Komodo | **Partial** | myAIAgent compose pins `v0.5.0@sha256:31e2...139c3a`. Production ops/Komodo state was not inspected in this continuation because the requested write scope was these two repos. |
| 4 | email-rs-owned scheduling; retire legacy IMAP/Gmail mail paths | **Complete in myAIAgent code** | `6a567c7` removed `imap_email` and Gmail mail ingestion; `GoogleConnector` is calendar-only. Real deployed behavior remains an ops check. |
| 5 | Encrypted credentials and safe HTML | **Complete in email-rs code** | AES-256-GCM migration/fail-closed behavior and safe HTML/remote-image blocking are landed. Production must supply a stable `EMAIL_RS_CREDENTIAL_KEY`. |
| 6 | Drafts, outgoing attachments, threading, pagination | **Partial, active dirty slice** | Outgoing attachments, provider-derived threading, read/download workflows, and folder/label/smart pagination are implemented locally. Provider-synchronized persistent draft create/update/delete is not implemented. Search pagination and physical mobile download behavior still need product validation. |
| 7 | Webhook-driven AI indexing and persisted triage | **Complete in code; deployment validation pending** | `6a567c7` plus `scripts/email_rs_acceptance.sh` implement the path. Production still needs webhook/secret configuration and a real provider message proof. |
| 8 | Real-provider and platform release validation | **Not complete** | No fresh proof for Gmail, Microsoft, generic IMAP, Windows installer lifecycle, or physical iOS/Android onboarding/secure storage. |

### Verification recorded in this continuation

Passed:

```text
email-rs: cargo fmt --check
email-rs: cargo clippy --workspace --all-targets -- -D warnings
email-rs: cargo test --workspace --locked
          10 email-client + 109 Fluent + 150 server tests passed
email-rs: cargo run -q -p email-server --bin export_openapi --locked
email-rs: generated Python client ruff/compile checks
email-rs: frontend npm run build
email-rs: mobile npm run build

myAIAgent: docker compose build backend-test
myAIAgent: focused tests/test_mail.py — 13 passed
myAIAgent: full backend pytest — 258 passed, 2 deselected
myAIAgent: frontend npm ci && npm run build
myAIAgent: Android ./gradlew testDebugUnitTest --rerun-tasks
```

The real `scripts/email_rs_acceptance.sh` was not rerun because it needs the
Forgejo token/release boundary and the current email-rs changes are not yet
published as a new immutable image/SDK.

### Exact next actions after interruption

1. Decide the next synchronized email-rs version for the post-v0.5 attachment,
   threading, pagination, and attachment-reading work; update Rust/OpenAPI/SDK,
   then regenerate the client and bundles once more.
2. Implement provider-synchronized draft create/update/delete, or leave row 6
   explicitly partial. Do not claim local browser autosave is synchronized
   draft support.
3. Publish/pin that next immutable SDK/image in an explicit myAIAgent dependency
   change, then rerun `scripts/email_rs_acceptance.sh`.
4. Separately perform the secrets/Komodo/provider/platform gates in the final
   section below. Those cannot be proven by unit tests in these worktrees.

## Superseded handoff from earlier on 2026-07-12

This section is retained as history only. Its status table predates the v0.5
release, the landed myAIAgent webhook/legacy-removal commit, and the current
uncommitted client work. Do not use it as the source of truth.

Last audited: **2026-07-12 UTC** against:

- `email-rs` at `62f9f9a` on `main` plus the uncommitted `v0.5.0` security/event work described below (`v0.4.0` is commit `45ec198`).
- `myAIAgent` at `076cb91` on `main`, plus the uncommitted webhook-indexing work listed below.

This section was the source of truth for an earlier continuation and is now
superseded by the first section in this document. Its commands and gaps are
retained only to explain how the current state was reached.

The answer to **"has all this work been completed?" is no**.

### Recommended-order status

| # | Recommendation | State | Current evidence / exact gap |
|---|---|---|---|
| 1 | Machine tokens, full OpenAPI, stable logical IDs, synchronized versioning | **Complete in code; v0.5 release pending** | `email-rs` commit `45ec198` established the v0.4 boundary. Current uncommitted work aligns every Rust crate, OpenAPI, and generated Python package at `0.5.0`; integration tokens remain hash-only/scoped and messages expose `logicalId`/`occurrenceId`. |
| 2 | Generated Python client and cross-repository acceptance test | **Partial** | `email-rs-client==0.4.0` is published and consumed by `myAIAgent` commit `076cb91`; the `0.5.0` client is generated and builds locally but is not published/consumed yet. `scripts/email_rs_acceptance.sh` exists and exercises a real pinned container, but is not a required CI gate and has not been extended through webhook indexing. |
| 3 | Pin sidecar in compose and Komodo; remove `:latest` | **Partial** | myAIAgent compose pins `repo.indexarr.net/indexarr/email-rs:v0.4.0@sha256:7cea...ba5f`. The current Komodo/ops production declaration and deployed digest still need an authoritative check; do not claim this item complete from local compose alone. |
| 4 | Let email-rs own sync scheduling; retire legacy IMAP/Gmail paths | **Partial** | myAIAgent no longer registers `EmailRsConnector` with its scheduler, and its `ingest()` is deliberately empty. Explicit user sync remains. Legacy `EmailConnector` and the Gmail portion of `GoogleConnector` are still registered and persist/index email, so retirement is not complete. Browser query refreshes still exist but do not themselves trigger provider sync. |
| 5 | Harden credential storage and HTML rendering | **Complete in code; deployment key pending** | Current v0.5 work encrypts OAuth JSON and IMAP/SMTP passwords with randomized AES-256-GCM envelopes, migrates legacy rows, and fails closed without the stable key. Web/mobile now sanitize dangerous HTML, block remote images with CSP, remove network-capable elements/attributes, and use referrer-free sandboxed frames; Fluent already uses its safe renderer. Production still needs `EMAIL_RS_CREDENTIAL_KEY` configured before rollout. |
| 6 | Drafts, outgoing attachments, threading, pagination | **Not complete** | Incoming attachment metadata/download exists. Persistent/synchronized drafts, outgoing attachment upload, provider thread IDs/reconciliation, and complete web/mobile pagination are not implemented end-to-end. |
| 7 | Webhook-driven AI indexing and persisted triage | **In progress (uncommitted)** | email-rs v0.5 now persists standard events and delivery status, retries with bounded backoff, exposes cursor replay, and emits create/update/move/delete/sync-completed events. myAIAgent has an HMAC receiver, durable receipts, logical mapping, unified persistence/search indexing, and triage/memory enqueueing. Focused tests pass in both repos. Neither side is committed/pushed/deployed; secrets and webhook registration are not configured. |
| 8 | Platform and real-provider release validation | **Not complete** | No current evidence proves Gmail, Microsoft, and generic IMAP acceptance plus Windows installer/upgrade/uninstall and physical iOS/Android onboarding/secure-storage validation. |

### Landed implementation evidence

`email-rs`:

- `a4b5573` added application authentication and the secure multi-client baseline.
- `45ec198` added scoped integration-token lifecycle, schema-bearing OpenAPI,
  logical/occurrence identities, aligned `0.4.0` versions, and the generated
  Python SDK.
- `62f9f9a` repaired the release Rust toolchain. Current `main` matches
  `origin/main` before the v0.5 work in this worktree.
- `/health`, `/ready`, `/version`, and `/capabilities` exist. `/ready` checks
  database access and the required migration version.
- Uncommitted v0.5 adds durable standard events, delivery retry/status, cursor
  replay, per-account sync locking, encrypted provider credentials, and safe
  web/mobile HTML rendering. Its generated Python SDK is also `0.5.0`.

`myAIAgent`:

- `076cb91` pins `email-rs-client==0.4.0`, uses the generated SDK with a scoped
  bearer token, pins the sidecar image tag and digest, removes email-rs from
  scheduled ingestion, and adds `scripts/email_rs_acceptance.sh`.
- Existing uncommitted CI edits supply Forgejo package credentials to lint/test
  and BuildKit without placing the token in the image or repository. Preserve
  and review these edits; they pre-date the webhook receiver work.

### Current uncommitted myAIAgent webhook slice

Do not overwrite or silently discard these shared-worktree changes:

- `backend/app/api/email_rs_webhooks.py`: HMAC-authenticated, idempotent webhook
  receiver at `POST /api/webhooks/email-rs`.
- `backend/app/models/email.py`: `EmailRsMessageLink` maps durable `logicalId`
  to the current `occurrenceId` and the unified `EmailMessage` AI state;
  `EmailRsWebhookEvent` records processing status.
- `backend/alembic/versions/020_email_rs_webhook_index.py`: production migration
  for those tables.
- `backend/app/main.py`, `backend/app/config.py`, `.env.example`, and compose:
  receiver routing plus `EMAIL_RS_WEBHOOK_SECRET`.
- `backend/tests/test_mail.py`: signature, persistence, indexing enqueue, and
  duplicate-delivery coverage.

Focused verification already performed:

```text
docker compose build backend-test                 PASS
docker compose run --rm backend-test \
  uv run pytest -q tests/test_mail.py \
  -k 'webhook or scoped_integration_token or connector_observes'
                                                  4 passed, 7 deselected
ruff format/check for the changed Python files    PASS
```

The direct host-side pytest attempt is not a product failure: this agent runs
inside a container where `localhost:5432` is not Docker's published host and the
compose database network is not directly reachable. Run database tests through
`backend-test`, as above.

### Historical next actions (superseded; do not execute as the current plan)

1. Commit, push, tag, and verify email-rs v0.5.0, including publication of the
   already-generated `email-rs-client==0.5.0` and immutable container image.
2. Update myAIAgent to the published v0.5 SDK/image through an explicit
   dependency commit and extend its real acceptance script through events.
3. Point an email-rs webhook at `/api/webhooks/email-rs`; configure
   `EMAIL_RS_CREDENTIAL_KEY` and the shared `EMAIL_RS_WEBHOOK_SECRET` through
   Infisical/Komodo, then run the real
   `EMAIL_RS_WEBHOOK_SECRET` through Infisical/Komodo, then run the real
   cross-repository acceptance test including delivery, retry, logical-ID
   reconciliation, persisted triage, and search visibility.
4. Remove the legacy IMAP email connector and Gmail-mail ingestion from
   myAIAgent without removing its independent calendar integration.
5. Complete the remaining production-client work listed in the table. Do not
   call the review complete until every row is
   complete and verified at its actual scope.

### Historical completion gates

Before changing the answer from "no" to "yes", record fresh results for:

```bash
# email-rs
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo run -q -p email-server --bin export_openapi --locked

# myAIAgent (inside its repo; package credentials required for uv/build)
docker compose build backend-test
docker compose run --rm backend-test uv run pytest -q
cd frontend && npm ci && npm run build

# real cross-repository boundary
FORGEJO_TOKEN=... scripts/email_rs_acceptance.sh
```

Also verify the Forgejo CI runs to green, the production ops/Komodo sidecar is
at the intended immutable digest, the webhook is registered with its secret,
and an actual provider message becomes searchable and receives persisted triage
in myAIAgent after an email-rs-owned sync.

## Original review snapshot (historical)

The remainder of this document is the 2026-07-11 review that generated the
worklist. Statements written in the present tense below describe that original
snapshot and may now be superseded. Use the authoritative handoff above for
current status.

### Bottom line

Keep email-rs standalone and do not put AI inside it. Treat it as an independently versioned email service and application with a stable API. myAIAgent should supply the AI, automation, and agent-tool layer.

The current integration is not copying Rust source. It is hand-copying email-rs's wire contract into Python dictionaries and TypeScript types. That contract has already drifted and is now incompatible across the current main branches.

email-rs is a strong email engine and credible beta application, but it is not yet a fully fledged production email client.

## Highest-priority findings

### 1. The current integration cannot authenticate

myAIAgent's adapter sends no email-rs authorization header, while email-rs now protects all mailbox routes with its own application session authentication.

This arose naturally from the development timeline: the myAIAgent integration landed on 2026-06-10, while email-rs authentication landed on 2026-07-11. The integration plan still describes email-rs as an unauthenticated internal service.

### 2. There is no suitable machine-to-machine authentication model

email-rs currently supports interactive user sessions. myAIAgent instead needs a revocable integration token with scopes such as:

- `mail.metadata.read`
- `mail.body.read`
- `mail.modify`
- `mail.send`
- `mail.accounts.read`
- `mail.sync`

The token should be created and managed in email-rs, stored there only as a hash, and supplied to myAIAgent through secrets management.

### 3. The OpenAPI document is only a route inventory

The current document lists paths, success and authentication responses, and the bearer security scheme. It does not describe request bodies, response schemas, query parameters, pagination, errors, or event payloads.

It therefore cannot be used to generate a reliable Python SDK or detect most breaking contract changes.

### 4. Message IDs are unsuitable as cross-application foreign keys

Archive and move operations delete the local mailbox occurrence and allow a later synchronization to create a new row. Consequently, keying myAIAgent's AI state only by the current email-rs message ID is unsafe.

email-rs should expose both:

- A durable logical message ID for cross-application references.
- A mailbox-occurrence ID for folder and IMAP UID operations.

RFC Message-ID alone is insufficient because it may be absent or duplicated. Gmail's stable provider IDs can help for Gmail, while generic IMAP needs a carefully documented reconciliation strategy.

### 5. There are three competing sync triggers

email-rs polls every 300 seconds, myAIAgent's scheduler triggers email-rs every 60 seconds, and the myAIAgent browser also triggers it every 60 seconds. There is no obvious per-account synchronization lock.

One component should own scheduling. The recommended owner is email-rs because it owns the provider connections and mailbox database. Consumers should request an exceptional on-demand sync or observe sync status, not maintain their own polling loops.

### 6. The planned webhook and indexing path was not built

myAIAgent has no email-rs webhook receiver or persistent mapping of email-rs logical messages to AI state. Its connector reports synchronized accounts as "indexed" even though it does not index the resulting messages.

Triage is currently computed on demand and is not persisted for email-rs messages.

## Recommended dependency model

Do not vendor email-rs source into myAIAgent and do not use a Git submodule.

Use a service and contract dependency:

```text
email-rs repository
  |-- standalone applications
  |-- versioned Docker image
  |-- complete OpenAPI contract
  `-- generated/versioned Python SDK
               |
               v
myAIAgent
  |-- pins email-rs image version/digest
  |-- depends on the email-rs Python SDK
  |-- stores an integration token
  `-- owns AI tools and AI-derived state
```

Concretely:

- Release email-rs with a synchronized product and API version. The crates currently report `0.1.0`, while repository tags reach `v0.3.9`.
- Pin an immutable image version or digest, never `:latest`.
- Publish a generated `email-rs-client` Python package to the Forgejo package registry.
- Add a compose profile for local development with both applications.
- Add a cross-repository acceptance test that starts a real email-rs container, bootstraps authentication, and exercises accounts, messages, search, and mutations.
- Upgrade email-rs through an explicit dependency pull request.
- Keep the Rust `email-client` and `email-types` crates for email-rs's own Rust clients. They do not solve the Python integration boundary.

Avoid embedding the full email-rs SPA in an iframe. An iframe creates cookie, CSP, authentication, routing, and visual-lifecycle coupling. Give myAIAgent a focused mail surface and an "Open full email client" link to email-rs.

## API surfaces email-rs should expose

The API should remain deterministic and non-AI-specific.

### Core read API

- Capabilities, version, readiness, and synchronization status.
- Accounts and folders.
- Cursor-paginated messages.
- Logical threads and their messages.
- Message metadata separately from body content.
- Attachments with metadata and streamed downloads.
- Search with stable pagination and structured filters.
- Labels, flags, snooze state, and drafts.

Separating message metadata from bodies is valuable for agent privacy: an integration can inspect sender, subject, and date without automatically retrieving private content.

### Mutations

- Mark read/unread and flag/unflag.
- Move, archive, trash, restore, and permanent delete.
- Label changes.
- Draft create, update, and delete.
- Send, reply, and forward.
- Attachment upload.
- Explicit sync job creation and job-status retrieval.

Send and destructive operations should support idempotency keys so retries cannot duplicate email or repeat mutations.

### Integration and event API

- Scoped integration-token lifecycle.
- Durable webhooks with event IDs, timestamps, retries, and delivery status.
- Replay or cursor endpoint for missed events.
- SSE or WebSocket updates for interactive clients.
- Stable events such as `message.created`, `message.updated`, `message.moved`, `message.deleted`, and `sync.completed`.

Agent tools should live in myAIAgent or an optional standalone `email-rs-mcp` adapter. For example, `search_email` maps to read scopes, while `send_email` remains an explicitly confirmed write action.

## Work remaining for a fully fledged email app

### Must address before calling it production-ready

- Encrypt OAuth refresh tokens and IMAP/SMTP passwords at rest. They are currently stored directly in SQLite.
- Sanitize email HTML and block remote images. The web and mobile clients currently put message HTML into iframe `srcDoc`; sandboxing scripts does not prevent tracking images and other remote resource requests.
- Add persistent drafts and draft synchronization.
- Add attachment upload and send.
- Implement real threading. Providers currently assign no thread ID, so the UI's thread grouping is largely aspirational.
- Add complete pagination or infinite scrolling. The backend paginates, but the web UI requests only the first page.
- Finish the web attachment workflow. The web reading pane currently shows only a generic attachment indicator.
- Add per-account sync locking, configurable cadence, timeout, backoff, jitter, and visible last-success/last-error state.
- Add durable webhook retry instead of fire-and-forget delivery.
- Add readiness checks covering the database and migration state.

### Product-completeness work

- Better address and header handling: CC/BCC, Reply-To, References, mailing-list headers, and richer recipient identity.
- Sent-message reconciliation and delivery/error reporting.
- Spam/junk and restore workflows.
- Offline behavior and cached mobile pagination.
- Mobile attachment handling, richer compose fields, and feature parity.
- Import/export and user-driven backup/restore.
- Accessibility, localization, and keyboard/screen-reader verification.
- Real-provider acceptance against Gmail, Microsoft, and generic IMAP.
- Windows installer, upgrade, tray, and uninstall validation.
- Physical iOS/Android onboarding and secure-storage validation.

The Fluent desktop also has documented editor limitations, including no arbitrary text selection, caret positioning, or undo history.

## Calendar ownership

email-rs should retain calendar support as part of its standalone application. myAIAgent should continue to own its own assistant-facing calendar integration and should initially consume only the email API from email-rs.

There is no need to force one application to become the system of record for both calendar experiences. If consolidation is desirable later, it should be a separate contract and data-ownership decision.

## Recommended order of work

1. Fix the API boundary: machine tokens, full OpenAPI, stable logical IDs, and versioning.
2. Add a generated Python client and cross-repository integration test.
3. Pin and declare the sidecar dependency in compose and Komodo; remove `:latest`.
4. Remove duplicate sync scheduling and retire myAIAgent's legacy IMAP/Gmail paths.
5. Harden email-rs credential storage and HTML rendering.
6. Complete drafts, outgoing attachments, threading, and pagination.
7. Add webhook-driven AI indexing and triage in myAIAgent.
8. Finish platform and real-provider release validation.

## Verification performed during the review

- `cargo check -p email-server --locked` passed.
- `cargo test -p email-server --locked` passed all 136 server tests.
- myAIAgent's non-database mail tests passed.
- The full myAIAgent mail test file could not run in the review environment because its required local PostgreSQL service was unavailable.
- Neither repository currently runs a real cross-application contract test.
