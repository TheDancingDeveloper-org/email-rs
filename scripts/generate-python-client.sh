#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
SPEC=$(mktemp)
trap 'rm -f "$SPEC"' EXIT

cd "$ROOT"
cargo run -q -p email-server --bin export_openapi --locked > "$SPEC"
if [ "${OPENAPI_SPEC_OUTPUT:-}" ]; then
  cp "$SPEC" "$OPENAPI_SPEC_OUTPUT"
fi
mkdir -p clients
uvx --from openapi-python-client==0.29.1 openapi-python-client generate \
  --path "$SPEC" \
  --config openapi-python-client.json \
  --meta uv \
  --output-path clients/python \
  --overwrite \
  --fail-on-warning
