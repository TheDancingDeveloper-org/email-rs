# Development and reproducible builds

## Rust workspace

The workspace contains:

- `email-types` — shared serializable API/domain types;
- `email-client` — typed Rust HTTP client used by the native desktop;
- `email-server` — Axum server, SQLite cache, providers, sync, and browser assets;
- `email-desktop-fluent` — native FluentGUI application.

All FluentGUI crates are fetched from `https://github.com/AusAgentSmith-org/fluent-gpui.git` at exact commit `f601e54b4e58e416bc7495a75468b82af9a10545`. `Cargo.lock` is generated from these canonical dependencies. A sibling checkout must never be needed by CI, Docker, or a clean clone.

## Linux dependencies

Ubuntu/Debian workspace checks and the Fluent desktop need:

```bash
sudo apt-get install pkg-config libssl-dev \
  libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev
```

## Required checks

Run from the repository root:

```bash
cargo metadata --locked --format-version 1 >/dev/null
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
docker build .
```

Frontend checks:

```bash
(cd frontend && npm ci && npm run build)
(cd mobile && npm ci && npm run build)
```

GitHub CI runs the Rust matrix in an explicitly named clean-checkout job and rejects manifests containing `../../../` path dependencies. A separate clean checkout builds the production server image.

## Optional local FluentGUI override

Developers changing both repositories may create `.cargo/config.toml`, which is ignored by this repository:

```toml
[patch."https://github.com/AusAgentSmith-org/fluent-gpui.git"]
fluent-app = { path = "../../FluentGUI/crates/fluent-app" }
fluent-core = { path = "../../FluentGUI/crates/fluent-core" }
fluent-layout = { path = "../../FluentGUI/crates/fluent-layout" }
fluent-primitives = { path = "../../FluentGUI/crates/fluent-primitives" }
fluent-test-utils = { path = "../../FluentGUI/crates/fluent-test-utils" }
gpui = { path = "../../FluentGUI/crates/gpui" }
```

Adjust paths for the local directory layout. Never copy this override into a committed manifest, and regenerate/check `Cargo.lock` after returning to canonical dependencies.

## Running locally

Native Fluent desktop:

```bash
cargo run -p email-desktop-fluent
```

Browser backend and Vite frontend:

```bash
# terminal 1
cargo run -p email-server

# terminal 2
cd frontend && npm run dev
```

Set `EMAIL_RS_BOOTSTRAP_PASSWORD` before the first server start. The default bind is loopback and does not require the insecure-development override. The checked-in Docker Compose stack binds the backend inside its container network and therefore sets `EMAIL_RS_ALLOW_INSECURE_HTTP=true` explicitly.

## Release outputs

- Docker builds `email-server` explicitly and includes the React browser assets.
- Linux release workflows build both `email-server` and `email-desktop-fluent`.
- The Windows MSI still packages the legacy `email-server.exe` desktop until Fluent authentication and platform parity gates pass.
