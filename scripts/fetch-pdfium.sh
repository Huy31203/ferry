#!/usr/bin/env bash
# Fetch the pinned PDFium build (bblanchon/pdfium-binaries, chromium/7881).
# Usage: scripts/fetch-pdfium.sh [target-triple]   (default: the host triple)
# Output: src-tauri/binaries/{libpdfium.dylib|pdfium.dll} and pdfium-NOTICE.txt
set -euo pipefail

BASE="https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F7881"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TRIPLE="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
CACHE="$ROOT/target/pdfium-download"
OUT_DIR="$ROOT/src-tauri/binaries"

case "$TRIPLE" in
  *-apple-darwin | universal-apple-darwin)
    ASSET=pdfium-mac-univ.tgz
    SHA256=df451a413c3609585e84a4a91110a9bc889cff05fe3b2db0ed817c9e90c3f7d3
    LIB=lib/libpdfium.dylib
    DEST=libpdfium.dylib
    ;;
  x86_64-pc-windows-msvc)
    ASSET=pdfium-win-x64.tgz
    SHA256=73cc0de638ac2095e7445bf56a38200a5b7c7ca0e9f4ba144598f2457377ac08
    LIB=bin/pdfium.dll
    DEST=pdfium.dll
    ;;
  *) echo "unsupported target: $TRIPLE" >&2; exit 1 ;;
esac

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

mkdir -p "$CACHE" "$OUT_DIR"
ARCHIVE="$CACHE/$ASSET"
[ -f "$ARCHIVE" ] || curl -fsSL "$BASE/$ASSET" -o "$ARCHIVE"
ACTUAL="$(sha256 "$ARCHIVE")"
if [ "$ACTUAL" != "${PDFIUM_SHA256:-$SHA256}" ]; then
  echo "pdfium archive checksum mismatch: got $ACTUAL" >&2
  # Drop the cached download so a corrupt file is fetched again next run.
  rm -f "$ARCHIVE"
  exit 1
fi

WORK="$CACHE/${ASSET%.tgz}"
rm -rf "$WORK" && mkdir -p "$WORK"
tar xzf "$ARCHIVE" -C "$WORK"
cp "$WORK/$LIB" "$OUT_DIR/$DEST"

# The licences travel with the library: PDFium's own, then each bundled project's.
{
  echo "== LICENSE =="
  cat "$WORK/LICENSE"
  for f in "$WORK"/licenses/*.txt "$WORK"/licenses/*.md; do
    [ -f "$f" ] || continue
    printf '\n== %s ==\n' "$(basename "$f")"
    cat "$f"
  done
} > "$OUT_DIR/pdfium-NOTICE.txt"
echo "fetched $OUT_DIR/$DEST"
