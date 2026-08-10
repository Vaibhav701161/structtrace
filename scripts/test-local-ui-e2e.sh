#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
server_log="$(mktemp)"
server_pid=""

cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -f "$server_log"
}
trap cleanup EXIT

cd "$repository_root"
cargo build --locked -p structtrace-cli
target/debug/structtrace open --no-browser >"$server_log" 2>&1 &
server_pid="$!"

local_url=""
for _ in $(seq 1 30); do
  local_url="$(sed -n 's/^StructTrace Local: //p' "$server_log" | head -n 1)"
  if [[ -n "$local_url" ]]; then
    break
  fi
  sleep 1
done

if [[ -z "$local_url" ]]; then
  cat "$server_log" >&2
  echo "StructTrace Local did not become ready within 30 seconds" >&2
  exit 1
fi

cd "$repository_root/ui"
STRUCTTRACE_UI_URL="$local_url" npm run test:e2e -- "$@"
