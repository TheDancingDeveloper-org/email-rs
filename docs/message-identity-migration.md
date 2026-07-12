# Message identity and migration

## Identity model

RFC `Message-ID` is correlation metadata, not an occurrence identifier. The same message can legitimately appear in multiple folders or accounts, and some messages have no RFC Message-ID.

An IMAP occurrence is uniquely identified by:

```text
(folder_id, uid_validity, uid)
```

`folder_id` belongs to one account, so this scopes the UID to both account and mailbox. `messages.message_id` is nullable, non-unique, and indexed for correlation/search.

The API exposes that concrete row as both the backward-compatible `id` and the
explicit `occurrenceId`. It separately exposes `logicalId`, a durable identifier
derived from the account and normalized message metadata. The logical identifier
survives a move that deletes one occurrence and later creates another.

The reconciliation key includes RFC Message-ID, sender, date, and subject. This
avoids treating RFC Message-ID as unique when providers duplicate it. When RFC
Message-ID is absent, the remaining metadata provides a documented best-effort
generic-IMAP identity; only a message with no identifying metadata falls back to
its occurrence ID. Future provider-native stable IDs can replace the generic
fingerprint without changing the occurrence model.

## Synchronization

- Metadata upserts use mailbox identity and retain the existing local message ID on conflict.
- All mutable metadata is refreshed on a repeated sync.
- Rules and new-message webhooks run only when an insert creates a new row.
- Each folder sync obtains the complete remote UID set separately from date-windowed metadata fetching and removes cached occurrences no longer present remotely.
- A UIDVALIDITY change transactionally clears the affected folder cache, stores the new value, and forces the folder to reconcile.
- Archive/move removes the stale source occurrence after the remote operation; the next sync discovers the destination with its server-assigned UID.

## Legacy migration

Before rebuilding a legacy messages table, email-rs uses SQLite `VACUUM INTO` to create a timestamped sibling backup. Deduplication, table rebuild, index recreation, and reconciliation marking occur transactionally after that backup succeeds. Existing local IDs are copied into the new table wherever the deduplicated row survives.

Tracking tables:

- `migration_backups` records the migration version, backup path, creation time, and `retained`/`releasable` state.
- `account_reconciliation` records accounts that must complete a successful post-migration folder pass.

When all folders for an account synchronize without errors, its reconciliation row receives `completed_at`. A backup becomes `releasable` only when it has no incomplete accounts. The application never deletes backup files automatically.

Before manual deletion:

1. Confirm startup completed the schema migration without error.
2. Confirm each account completed reconciliation and folder/message counts are plausible.
3. Confirm the backup is marked `releasable`.
4. Retain an independent database backup according to the operator's normal policy.
5. Delete the timestamped migration backup manually only after those checks.
