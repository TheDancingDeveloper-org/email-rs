# Changelog

## Unreleased

## 0.7.2

- 0.7.1 was tagged but its release never published: the Windows zip packaging broke. 0.7.2 has the same content as 0.7.1, and the release packaging fix (#116) is included.

## 0.7.1

- Webhook deliveries keep the legacy body-only `X-Email-Signature` (deprecated) and add `X-EmailRs-Timestamp` plus `X-EmailRs-Signature-V2` (`{timestamp}.{body}` HMAC). See docs/deployment-security.md. Receivers that verify only the body should move to the v2 headers.
- Webhook SSRF checks now block IPv6 documentation addresses and judge the IPv4 embedded in NAT64, IPv4-compatible, 6to4 and Teredo addresses. The delivery client ignores `HTTP(S)_PROXY` and `ALL_PROXY`.
- Security: SQLite data files are owner-only (0700 directory, 0600 files), and startup fails rather than falling back to `/tmp` for mail data. Message-identity backups are created at mode 0600 and refuse existing paths or symlinks.
- Security: Microsoft accounts are bound to tenant and object id, Google identities must be verified, and an OAuth reconnect that would re-point an account to a different mailbox is refused.
- Security: `ClientCredential` no longer prints its bearer token in `Debug`, and the per-instance client credential no longer races a process-wide default.
- Release binaries: the release workflow publishes `email-server` for Linux amd64 and arm64 and Windows, with one combined `SHA256SUMS` and an SBOM, and publishes only after the binaries are built. The release dry run runs the same builds and publishes nothing.
- CI: release binaries are cross-built with a pinned, checksummed cargo-zigbuild and zig fetched into the runner temp directory, so shared runners' apt and dpkg state is no longer changed.
- email-ui: the fluent2-egui desktop shell starts with sign-in, session restore on launch, sign-out, and a folders, message list and reading pane view. Remote servers must use `https://` (loopback excepted), and secrets are redacted in `Debug` output.
- email-client: the server URL is parsed with the `url` crate, and redirects are followed only to `https`, or to `http` on loopback, so a bearer token is not sent over a downgrade.

## 0.7.0

- Upstream myaiagent's email-rs history (25 commits) into this repository, including body cleaning and search, provider connect scope, and parallel sync write locks.
- WI-587: drop leftover myaiagent tracker ids and cap OAuth error text.
- Publish a versioned `ghcr.io/thedancingdeveloper-org/email-rs` image and the Python client package on `v*` tags, with a fork-guarded release dry run on release-path PRs and nightly.
- CI: org runner-policy gate, a single required `ci` check, and a clean-checkout Rust toolchain of 1.95.0.

## 0.6.2

Previous release.
