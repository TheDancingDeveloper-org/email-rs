# email-rs desktop on fluent-gpui: parity and delivery plan

Status: operator decision recorded 2026-11-03. The native desktop application
continues on GPUI in `crates/email-desktop-fluent`; the proposed fluent2-egui
native/wasm replacement is cancelled. Tracking issue: [#130].

This assessment compares the Graft `backlog-1` checkout at `c7da5d7`, the GPUI
app and `docs/fluentgui-feature-tracker.md` with
`thedancingdeveloper/fluent-gpui` `main` at `2b773063`. The old fluent2 port
plan remains historical context, not the delivery plan.

[#130]: https://github.com/TheDancingDeveloper-org/email-rs/issues/130

## 1. Decision and boundaries

- Keep and finish `email-desktop-fluent`; do not create a replacement egui app
  or retire GPUI. React web and Capacitor mobile remain supported separately.
- Keep one Axum backend and API contract. Desktop continues to run
  `email_server::run_server` in-process and uses `email-client`.
- App behavior belongs in email-rs. Reusable missing controls or framework
  contracts become separate fluent-gpui work items.
- Adopt the fluent-gpui 3.x contract: one source revision for all fluent
  crates, and no direct `gpui` dependency once `fluent-core` re-exports land.
  G0 may temporarily use current `main`, because the first 3.x tag and complete
  re-export contract do not exist yet.

## 2. Current state

The 18,000-line native app is not a prototype. It already provides:

- a three-pane `Workspace`, resizable persisted panes, native title bar,
  account/folder tree, message grouping, virtualised list and unread status;
- asynchronous loading, first-run and add-account flows, manual IMAP/SMTP,
  Microsoft app-password and Gmail/Microsoft OAuth launch, plus account,
  rules, webhook and folder settings;
- message and bulk actions, labels, snooze, attachments, safe lossy HTML
  rendering, external-image stripping, and SSE sync status/refresh;
- compose/reply/reply-all/forward, contact suggestions, signatures,
  attachments, autosaved/provider drafts, discard confirmation and errors;
- search suggestions/results, advanced conditions, command palette, calendar
  week/event views, linked mail, theme/density, toasts and keyboard handling.

The tracker records 93/112 parity rows. Remaining explicit platform blockers
are Windows single-instance/foreground, close-to-tray, real OAuth callback
walkthroughs, packaging-specific Linux behavior and end-to-end platform/a11y
validation. Its 2026-05-27 backend summary is stale and must be reconciled with
the implementation during G1.

### Current fluent-gpui components

Current main supplies `Workspace`, panes/docks, tree, variable-height
`VirtualList`, data table, dialogs/drawers, menus, toolbar, toasts, command
palette, `DocumentViewer`/`RichDocument`, calendar/date-time picker, token
input, search box, chips, badges, cards, progress, title bar, native pickers and
AccessKit snapshots through `fluent-test-utils`.

Two reusable capabilities required by the backlog are absent:

1. **Grouped virtual list.** `VirtualList` has lazy variable-height rows,
   overscan and reveal, but not sticky headers, group semantics or expandable
   groups. Add these in fluent-gpui for #96 before richer conversation work.
2. **Inter-pane drag/drop.** There is no fluent-level source/target contract
   with ghost, modifiers and keyboard alternative. Add it in fluent-gpui for
   #97 before email-rs #81.

`DocumentViewer` is deliberately lossy. email-rs keeps sanitisation, blocked
image and link-risk policy. Only if G1 proves focusable links or a cancellable
link callback cannot be expressed should a narrow fluent-gpui hook be filed.

## 3. Gap list against parity

| Gap | Current state | Owner |
|---|---|---|
| Maintained dependency | All GUI dependencies pin `AusAgentSmith-org` `f601e54`; current 3.x is breaking | email-rs G0 |
| Tracker/test baseline | Tracker is stale; GUI tests need current test utilities and a11y snapshots | email-rs G1/#2 |
| Undo safety | Archive/delete/move/send act immediately | #76, #84 |
| Conversation/list | No stacked reader, quote folding, quick reply or reusable sticky groups | #77; fluent-gpui #96 |
| Keyboard | No registry, sequences, help sheet, F6 cycle or rebinding | #78 |
| Trust/privacy | Remote content is stripped, but allowlist/count and auth/link-risk UI are absent | #79, #80, #91 |
| Drag/drop | No message-to-folder/label action or reusable primitive | fluent-gpui #97, then #81 |
| Export/print | No source view, `.eml` save or print path | #83 |
| Compose depth | Core compose exists; undo-send, identities and quick reply remain | #84 |
| Preview | Safe attachment open/download exists; preview/progress remain | #85 |
| Settings/accounts | Preference depth, autodiscovery and invite RSVP remain | #86, #87 |
| Localisation | Strings/dates remain English-centric | #92, #95 |
| Lifecycle | Embedded server works; Windows single-instance, foreground and tray remain | G4/#5 |
| Distribution | Signing, deb/AppImage, update check, notifications and macOS decision remain | #88, #89, #93 |
| Documentation | No consolidated setup/troubleshooting/backup guide | #90 |
| Accessibility | Library snapshots exist; app gate and NVDA/Orca/high-contrast audits remain | G1/G4 |
| Crypto | Signature trust and S/MIME/OpenPGP remain server/product work | #91, #94 |

## 4. Ordered plan

### G0 — adopt current fluent-gpui and restore the build

Re-point every `fluent-*`, `gpui` and `fluent-test-utils` dependency from the
old fork/revision to one commit from `thedancingdeveloper/fluent-gpui` current
`main`. Migrate imports/APIs and remove obsolete workarounds without adding
features.

Exit: `cargo check -p email-desktop-fluent` and focused tests pass; all GUI git
dependencies use one maintained revision. Record temporary direct-`gpui` or
pre-tag exceptions for removal after the first 3.x tag.

### Library prerequisites — fluent-gpui

In parallel with G0: (1) grouped/sticky/expandable behavior on `VirtualList`
with AccessKit group semantics (#96); (2) reusable inter-pane drag/drop with a
ghost, modifiers and keyboard equivalent (#97). Do not invent a broad HTML
widget rewrite; file a narrow link hook only when G1 demonstrates the need.

### G1 — shell, read path and quality gate

Refresh the tracker after G0, migrate GUI tests and add AccessKit snapshots.
The egui/kittest-specific #82 closes, while its product requirements move here.
Then deliver #76–#81 and #83: undo; privacy/trust; shortcuts; conversation;
drag/drop after its prerequisite; source/export/print. Keep Gmail/Microsoft
OAuth as explicit manual gates.

Exit: mail is triaged by keyboard or pointer, destructive actions are
recoverable, hostile fixtures cannot bypass policy, and focused/a11y tests pass.

### G2 — compose and attachments

Complete #84–#85: undo-send, identities, quick reply, preview/progress and
inline images while preserving draft restart/provider sync and size limits.
Exit: send/receive/reply/forward, restart, undo and safe preview pass fixtures
plus one real-account walkthrough.

### G3 — settings, accounts and calendar

Complete #86–#87 and the app portion of #92: preferences, allowlist,
autodiscovery, provider setup, invite RSVP, string catalogue and locale dates.
Keep search, rules, webhooks, command palette and calendar working. Exit: every
non-platform tracker row is tested; credentialed flows have manual evidence.

### G4 — lifecycle, release and signoff

Finish Windows single-instance foregrounding, close-to-tray and embedded-server
shutdown (#5). Deliver #88–#90 and #93: signed Windows/Linux packages,
deb/AppImage, update check, checksums/SBOM, notifications/tray badge, docs,
macOS and opt-in crash-reporting decisions. Run NVDA, Orca, high-contrast and
manual Windows checks.

Exit: packaged native builds pass the refreshed parity checklist; signatures,
checksums and SBOM verify; install/update/uninstall and lifecycle checks pass on
Windows and Linux. GPUI is retained.

Post-G4: #91, #94 and #95 remain applicable, with security/server review, but
do not block first GPUI parity unless separately promoted.

## 5. Issue mapping

| Issue(s) | GPUI mapping |
|---|---|
| #1 | Close: egui/wasm spike superseded by decision and G0 |
| #2 | G1 shell/read-path stabilisation on existing GPUI app |
| #3 | G2 compose depth on existing composer |
| #4 | G3 settings/accounts/calendar depth on existing views |
| #5 | G4 lifecycle/release; retain, not retire, GPUI |
| #6 | Close: wasm desktop cutover is outside the native plan |
| #76–#81, #83 | G1; #81 waits for fluent-gpui #97 |
| #82 | Close kittest wording; carry a11y outcomes into G1/G4 with `fluent-test-utils`, NVDA and Orca |
| #84–#85 | G2 |
| #86–#87 | G3 |
| #88–#90, #93 | G4 |
| #91–#92 | GPUI app/server work; #92 starts G3, #91 may follow G4 |
| #94–#95 | Post-G4 |
| #96–#97 | Re-scope from fluent2-egui to fluent-gpui prerequisites |

Vogt items for G0 and the two library prerequisites are the small one-worker
delivery units created with this plan.
