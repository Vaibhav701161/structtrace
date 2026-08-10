#!/bin/sh
set -eu

repository="Vaibhav701161/structtrace"
version="${STRUCTTRACE_VERSION:-latest}"
install_dir="${STRUCTTRACE_INSTALL_DIR:-${HOME}/.local/bin}"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version) [ "$#" -ge 2 ] || { echo "--version requires a tag such as v1.0.0" >&2; exit 2; }; version="$2"; shift 2 ;;
    --uninstall) rm -f "$install_dir/structtrace"; echo "Removed $install_dir/structtrace"; exit 0 ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
  esac
done

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target="x86_64-unknown-linux-musl" ;;
  Darwin-x86_64) target="x86_64-apple-darwin" ;;
  Darwin-arm64|Darwin-aarch64) target="aarch64-apple-darwin" ;;
  *) echo "StructTrace has no prebuilt binary for $(uname -s) $(uname -m)." >&2; exit 1 ;;
esac

command -v curl >/dev/null 2>&1 || { echo "curl is required." >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then
  checksum_command="sha256sum -c"
elif command -v shasum >/dev/null 2>&1; then
  checksum_command="shasum -a 256 -c"
else
  echo "sha256sum or shasum is required." >&2
  exit 1
fi

temporary_dir="$(mktemp -d)"
trap 'rm -rf "$temporary_dir"' EXIT INT TERM
asset="structtrace-${target}.tar.gz"
if [ "$version" = "latest" ]; then
  base_url="https://github.com/${repository}/releases/latest/download"
else
  base_url="https://github.com/${repository}/releases/download/${version}"
fi

curl --fail --location --silent --show-error "$base_url/$asset" -o "$temporary_dir/$asset"
curl --fail --location --silent --show-error "$base_url/$asset.sha256" -o "$temporary_dir/$asset.sha256"
(cd "$temporary_dir" && $checksum_command "$asset.sha256")
if command -v gh >/dev/null 2>&1; then
  gh attestation verify "$temporary_dir/$asset" --repo "$repository"
elif [ "${STRUCTTRACE_REQUIRE_ATTESTATION:-0}" = "1" ]; then
  echo "GitHub CLI is required when STRUCTTRACE_REQUIRE_ATTESTATION=1." >&2
  exit 1
else
  echo "GitHub CLI not found; SHA-256 verified, provenance verification skipped."
fi
tar -xzf "$temporary_dir/$asset" -C "$temporary_dir"
mkdir -p "$install_dir"
install -m 0755 "$temporary_dir/structtrace" "$install_dir/structtrace"
"$install_dir/structtrace" --version
echo "Installed StructTrace to $install_dir/structtrace"
case ":${PATH}:" in
  *":${install_dir}:"*) ;;
  *)
    profile_file="${HOME}/.profile"
    path_line="export PATH=\"${install_dir}:\$PATH\""
    if ! grep -F "$path_line" "$profile_file" >/dev/null 2>&1; then
      printf '\n# StructTrace installer\n%s\n' "$path_line" >> "$profile_file"
    fi
    echo "Added $install_dir to PATH in $profile_file; open a new shell or run: $path_line"
    ;;
esac
echo "Uninstall with: rm $install_dir/structtrace"
echo "Update by rerunning this installer with --version <tag>."
