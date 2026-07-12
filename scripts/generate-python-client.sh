#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
SPEC=$(mktemp)
trap 'rm -f "$SPEC"' EXIT

cd "$ROOT"
cargo run -q -p email-server --bin export_openapi --locked > "$SPEC"
mkdir -p clients
uvx openapi-python-client generate \
  --path "$SPEC" \
  --config openapi-python-client.json \
  --meta uv \
  --output-path clients/python \
  --overwrite \
  --fail-on-warning
