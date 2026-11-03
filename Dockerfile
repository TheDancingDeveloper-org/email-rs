# ── Stage 1: Build frontend ─────────────────────────���──────────────────────────
FROM mirror.gcr.io/library/node:22-alpine@sha256:fe4f6d74e20370acb04ad889077d14d0f64a9c8ea6a42bc60cddf1e844699914 AS frontend-build
WORKDIR /build
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ ./
COPY shared/ /shared/
# Build into /dist (override vite outDir to avoid needing the crates tree)
RUN npx vite build --outDir /dist --emptyOutDir

# ── Stage 2: Build Rust backend ────────────────────────────────────────────────
FROM mirror.gcr.io/library/rust:1.98-slim-bookworm@sha256:ff521445a372125ed4f76e1453a1f8098f2d05332d1601d30db1c1f62757e730 AS rust-build
RUN apt-get update && apt-get install -y ca-certificates pkg-config && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY rust-toolchain.toml Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
# A stalled crates download once hung CI inside BuildKit: bound each request,
# retry, and fetch in its own layer before the build.
ENV CARGO_HTTP_TIMEOUT=60 \
    CARGO_NET_RETRY=5 \
    CARGO_HTTP_LOW_SPEED_LIMIT=10
RUN cargo fetch --locked
RUN CARGO_BUILD_JOBS=4 cargo build --release --locked -p email-server

# ── Stage 3: Runtime ─────────────────────────────────��─────────────────────────
FROM mirror.gcr.io/library/debian:bookworm-slim@sha256:99c0b25c46d0214d5a2091092e7908907dcaecd6a962cde0ac700acee222b6f3
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && groupadd --gid 10001 email-rs \
    && useradd --uid 10001 --gid email-rs --no-create-home --shell /usr/sbin/nologin email-rs \
    && install -d -o email-rs -g email-rs -m 0700 /data \
    && rm -rf /var/lib/apt/lists/*

COPY --from=rust-build /app/target/release/email-server /usr/local/bin/email-server
COPY --from=frontend-build /dist /app/static

ENV HOST=0.0.0.0
ENV PORT=3000
ENV FRONTEND_DIST=/app/static
ENV DATABASE_URL=sqlite:///data/email.db

EXPOSE 3000
VOLUME ["/data"]
USER email-rs

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl --fail --silent --show-error http://127.0.0.1:3000/ready || exit 1

CMD ["email-server"]
