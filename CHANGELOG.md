# Changelog

## 0.7.0

- Upstream myaiagent's email-rs history (25 commits) into this repository, including body cleaning and search, provider connect scope, and parallel sync write locks.
- WI-587: drop leftover myaiagent tracker ids and cap OAuth error text.
- Publish a versioned `ghcr.io/thedancingdeveloper-org/email-rs` image and the Python client package on `v*` tags, with a fork-guarded release dry run on release-path PRs and nightly.
- CI: org runner-policy gate, a single required `ci` check, and a clean-checkout Rust toolchain of 1.95.0.

## 0.6.2

Previous release.
