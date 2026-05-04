# ── Stage 1: Build frontend ─────────────────────────���──────────────────────────
FROM node:22-alpine AS frontend-build
WORKDIR /build
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ ./
# Build into /dist (override vite outDir to avoid needing the crates tree)
RUN npx vite build --outDir /dist --emptyOutDir

# ── Stage 2: Build Rust backend ────────────────────────────────────────────────
FROM rust:1.85-slim AS rust-build
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
RUN cargo build --release -p email-server

# ── Stage 3: Runtime ─────────────────────────────────��─────────────────────────
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=rust-build /app/target/release/email-server /usr/local/bin/email-server
COPY --from=frontend-build /dist /app/static

ENV HOST=0.0.0.0
ENV PORT=3000
ENV FRONTEND_DIST=/app/static

EXPOSE 3000
VOLUME ["/data"]

CMD ["email-server"]
