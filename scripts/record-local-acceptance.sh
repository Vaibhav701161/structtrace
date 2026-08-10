#!/usr/bin/env bash
set -uo pipefail

audit_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
audit_output="${1:-$audit_root/acceptance/local-release-audit.json}"
audit_log_dir="$audit_root/target/local-acceptance"
audit_checks_file="$(mktemp)"
audit_failed=0

mkdir -p "$audit_log_dir"
printf '[]\n' >"$audit_checks_file"

record_check() {
  local audit_name="$1"
  local audit_command="$2"
  local audit_artifact_paths="${3:-}"
  local audit_log="$audit_log_dir/${audit_name}.log"
  local audit_status
  local audit_log_hash
  local audit_next

  (
    cd "$audit_root"
    bash -lc "$audit_command"
  ) >"$audit_log" 2>&1
  audit_status=$?
  audit_log_hash="$(sha256sum "$audit_log" | awk '{print $1}')"
  audit_next="$(mktemp)"
  jq \
    --arg name "$audit_name" \
    --arg command "$audit_command" \
    --arg log_sha256 "$audit_log_hash" \
    --arg artifact_paths "$audit_artifact_paths" \
    --argjson exit_code "$audit_status" \
    '. + [{name: $name, command: $command, exit_code: $exit_code, log_sha256: $log_sha256, artifact_paths: ($artifact_paths | split(",") | map(select(length > 0)))}]' \
    "$audit_checks_file" >"$audit_next"
  mv "$audit_next" "$audit_checks_file"
  if [[ "$audit_status" -ne 0 ]]; then
    audit_failed=1
  fi
}

audit_source_commit="$(git -C "$audit_root" rev-parse HEAD)"
if [[ -z "$(git -C "$audit_root" status --porcelain)" ]]; then
  audit_clean_before=true
else
  audit_clean_before=false
fi

record_check fmt 'cargo fmt --all -- --check'
record_check clippy 'cargo clippy --workspace --all-targets --all-features -- -D warnings'
record_check tests 'cargo test --workspace --all-features --locked'
record_check release_build 'cargo build --release --workspace --locked' 'target/release/structtrace'
record_check documentation 'mdbook build' 'docs/book/index.html'
record_check release_cli_help 'target/release/structtrace --help' 'target/release/structtrace'

mkdir -p "$(dirname "$audit_output")"
jq -n \
  --arg schema_version "1" \
  --arg generated_at "$(date --utc +%Y-%m-%dT%H:%M:%SZ)" \
  --arg source_commit "$audit_source_commit" \
  --arg rustc "$(rustc --version)" \
  --arg cargo "$(cargo --version)" \
  --arg platform "$(uname -srm)" \
  --argjson worktree_clean_before "$audit_clean_before" \
  --argjson checks "$(<"$audit_checks_file")" \
  '{
    schema_version: ($schema_version | tonumber),
    generated_at: $generated_at,
    source_commit: $source_commit,
    worktree_clean_before: $worktree_clean_before,
    environment: {rustc: $rustc, cargo: $cargo, platform: $platform},
    checks: $checks,
    passed: ($checks | all(.exit_code == 0))
  }' >"$audit_output"

printf 'Wrote %s\n' "$audit_output"
exit "$audit_failed"
