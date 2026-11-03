# GPUI desktop G1 delivery plan — November 2026

## Shortcut registry (#78)

The native shell keeps all global bindings in `bootstrap.rs`. The registry is
validated for duplicate keystrokes and exposes a coverage table for the Gmail
and Superhuman conventions. The `?` binding opens the compact in-app help
sheet; actions remain dispatched through GPUI so reader-local controls can
consume the same actions without duplicating key parsing.

## Message power tools (#83)

The reader action row provides view-source, save-as-`.eml`, and print. Export
is deliberately isolated in `export.rs`: source generation uses CRLF MIME
headers, save writes a deterministic `.eml`, and print writes escaped,
sanitised text HTML before handing it to the platform browser. The round-trip
test checks headers and body line endings.

Manual release checks remain: verify the save/open path on Windows, verify the
system-browser print route on Windows and Linux, and complete NVDA/Orca
accessibility passes on the signed packages.
