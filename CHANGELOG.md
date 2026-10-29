# Changelog

## Unreleased

- Webhook deliveries keep the legacy body-only `X-Email-Signature` (deprecated) and add `X-EmailRs-Timestamp` plus `X-EmailRs-Signature-V2` (`{timestamp}.{body}` HMAC). See docs/deployment-security.md. Receivers that verify only the body should move to the v2 headers.
- Webhook SSRF checks now block IPv6 documentation addresses and judge the IPv4 embedded in NAT64, IPv4-compatible, 6to4 and Teredo addresses. The delivery client ignores `HTTP(S)_PROXY` and `ALL_PROXY`.


## 0.7.0

- Upstream myaiagent's email-rs history (25 commits) into this repository, including body cleaning and search, provider connect scope, and parallel sync write locks.
- WI-587: drop leftover myaiagent tracker ids and cap OAuth error text.
- Publish a versioned `ghcr.io/thedancingdeveloper-org/email-rs` image and the Python client package on `v*` tags, with a fork-guarded release dry run on release-path PRs and nightly.
- CI: org runner-policy gate, a single required `ci` check, and a clean-checkout Rust toolchain of 1.95.0.

## 0.6.2

Previous release.
