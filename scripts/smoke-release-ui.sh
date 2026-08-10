#!/usr/bin/env bash
set -euo pipefail

smoke_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
smoke_binary="${1:-target/release/structtrace}"
smoke_log="$(mktemp)"
smoke_headers="$(mktemp)"
smoke_pid=""

cleanup() {
  if [[ -n "$smoke_pid" ]]; then
    kill "$smoke_pid" 2>/dev/null || true
    wait "$smoke_pid" 2>/dev/null || true
  fi
  rm -f "$smoke_log" "$smoke_headers"
}
trap cleanup EXIT

cd "$smoke_root"
"$smoke_binary" open --no-browser >"$smoke_log" 2>&1 &
smoke_pid="$!"
smoke_url=""
for _ in $(seq 1 30); do
  smoke_url="$(sed -n 's/^StructTrace Local: //p' "$smoke_log" | head -n 1)"
  [[ -n "$smoke_url" ]] && break
  sleep 1
done
if [[ -z "$smoke_url" ]]; then
  cat "$smoke_log" >&2
  echo "Packaged StructTrace Local did not become ready within 30 seconds" >&2
  exit 1
fi

curl --fail --silent --show-error "${smoke_url}api/v1/system" | jq -e '.product == "StructTrace" and .localOnly == true and .telemetry == false' >/dev/null
curl --fail --silent --show-error "${smoke_url}runs/direct-refresh-check" | grep -q 'assets/app.js'
curl --fail --silent --show-error --dump-header "$smoke_headers" --output /dev/null "${smoke_url}assets/app.js"
grep -Eiq '^content-type: (text|application)/javascript' "$smoke_headers"
curl --fail --silent --show-error --dump-header "$smoke_headers" --output /dev/null "${smoke_url}assets/structtrace-logo-mark.svg"
grep -Eiq '^content-type: image/svg\+xml' "$smoke_headers"
