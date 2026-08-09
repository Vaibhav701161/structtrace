#!/bin/sh
set -eu

repository="Vaibhav701161/structtrace"
version="${STRUCTTRACE_VERSION:-latest}"
install_dir="${STRUCTTRACE_INSTALL_DIR:-${HOME}/.local/bin}"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target="x86_64-unknown-linux-gnu" ;;
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
tar -xzf "$temporary_dir/$asset" -C "$temporary_dir"
mkdir -p "$install_dir"
install -m 0755 "$temporary_dir/structtrace" "$install_dir/structtrace"
"$install_dir/structtrace" --version
echo "Installed StructTrace to $install_dir/structtrace"
echo "Uninstall with: rm $install_dir/structtrace"
