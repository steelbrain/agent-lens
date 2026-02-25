#!/usr/bin/env bash
# Build release binaries for all supported platforms.
# Requires: cross (cargo install cross --git https://github.com/cross-rs/cross), Docker, macOS host.
set -euo pipefail

BIN="agent-lense"
DIST="dist"

MACOS_TARGETS=(
  x86_64-apple-darwin
  aarch64-apple-darwin
)

CROSS_TARGETS=(
  x86_64-unknown-linux-gnu
  aarch64-unknown-linux-gnu
  x86_64-pc-windows-gnu
)

# Map rust target triple to a short archive stem (no extension).
archive_stem() {
  case "$1" in
    x86_64-apple-darwin)         echo "${BIN}-x86_64-darwin" ;;
    aarch64-apple-darwin)        echo "${BIN}-aarch64-darwin" ;;
    x86_64-unknown-linux-gnu)    echo "${BIN}-x86_64-linux" ;;
    aarch64-unknown-linux-gnu)   echo "${BIN}-aarch64-linux" ;;
    x86_64-pc-windows-gnu)       echo "${BIN}-x86_64-windows" ;;
    *)                           echo "${BIN}-${1}" ;;
  esac
}

# Return the filename produced by cargo inside target/<triple>/release.
bin_filename() {
  case "$1" in
    *-windows-*) echo "${BIN}.exe" ;;
    *)           echo "${BIN}" ;;
  esac
}

# Package a built binary into an archive with LICENSE and README.
# Windows targets get .zip, everything else gets .tar.gz.
package() {
  local target="$1"
  local stem
  stem="$(archive_stem "${target}")"
  local bin
  bin="$(bin_filename "${target}")"
  local staging="${DIST}/${stem}"

  mkdir -p "${staging}"
  cp "target/${target}/release/${bin}" "${staging}/${bin}"
  cp LICENSE README.md "${staging}/"

  case "${target}" in
    *-windows-*)
      (cd "${DIST}" && zip -rq "${stem}.zip" "${stem}")
      ;;
    *)
      tar -czf "${DIST}/${stem}.tar.gz" -C "${DIST}" "${stem}"
      ;;
  esac

  rm -rf "${staging}"
}

if ! command -v cross &>/dev/null; then
  echo "error: 'cross' is not installed."
  echo "Install it with: cargo install cross --git https://github.com/cross-rs/cross"
  exit 1
fi

rm -rf "${DIST}"
mkdir -p "${DIST}"

echo "==> Building macOS targets (cargo)"
for target in "${MACOS_TARGETS[@]}"; do
  echo "  -> ${target}"
  rustup target add "${target}" &>/dev/null
  cargo build --release --target "${target}"
  package "${target}"
done

echo "==> Building Linux & Windows targets (cross)"
for target in "${CROSS_TARGETS[@]}"; do
  echo "  -> ${target}"
  cross build --release --target "${target}"
  package "${target}"
done

echo "==> Archives:"
ls -lh "${DIST}/"

echo ""
echo "==> SHA-256 checksums:"
shasum -a 256 "${DIST}"/*.{tar.gz,zip} 2>/dev/null

echo ""
echo "==> Done"
