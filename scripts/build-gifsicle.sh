#!/usr/bin/env bash
# Build the gifsicle sidecar from pinned, checksum-verified source.
# Usage: scripts/build-gifsicle.sh [target-triple]   (default: the host triple)
# Output: src-tauri/binaries/gifsicle-<triple>[.exe]
set -euo pipefail

VERSION=1.98
SHA256=e9d4cc0816db95c64d32e79f778836db0f1f7eb15868490bd330104e5146b66e
URL="https://www.lcdf.org/gifsicle/gifsicle-${VERSION}.tar.gz"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TRIPLE="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
CACHE="$ROOT/target/gifsicle-build"
TARBALL="$CACHE/gifsicle-${VERSION}.tar.gz"
WORK="$CACHE/$TRIPLE"
OUT_DIR="$ROOT/src-tauri/binaries"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

mkdir -p "$CACHE" "$OUT_DIR"
[ -f "$TARBALL" ] || curl -fsSL "$URL" -o "$TARBALL"
ACTUAL="$(sha256 "$TARBALL")"
if [ "$ACTUAL" != "${GIFSICLE_SHA256:-$SHA256}" ]; then
  echo "gifsicle tarball checksum mismatch: got $ACTUAL" >&2
  # Drop the cached download so a corrupt file is fetched again next run.
  rm -f "$TARBALL"
  exit 1
fi

rm -rf "$WORK" && mkdir -p "$WORK"
tar xzf "$TARBALL" -C "$WORK" --strip-components=1
cd "$WORK"

case "$TRIPLE" in
  universal-apple-darwin) ARCHS="-arch x86_64 -arch arm64" ;;
  x86_64-apple-darwin) ARCHS="-arch x86_64" ;;
  aarch64-apple-darwin) ARCHS="-arch arm64" ;;
  x86_64-pc-windows-msvc) ARCHS="" ;;
  x86_64-unknown-linux-gnu) ARCHS="" ;;
  *) echo "unsupported target: $TRIPLE" >&2; exit 1 ;;
esac

case "$TRIPLE" in
  *-apple-darwin)
    ./configure --disable-gifview --disable-gifdiff \
      CFLAGS="-O2 $ARCHS -mmacosx-version-min=13.0" >/dev/null
    make -j4 >/dev/null
    BIN="src/gifsicle"
    EXT=""
    ;;
  x86_64-unknown-linux-gnu)
    ./configure --disable-gifview --disable-gifdiff CFLAGS="-O2" >/dev/null
    make -j4 >/dev/null
    BIN="src/gifsicle"
    EXT=""
    ;;
  x86_64-pc-windows-msvc)
    # Needs the MSVC environment (ilammy/msvc-dev-cmd in CI). Put MSVC's own
    # directory first so its link.exe wins over Git Bash's /usr/bin/link.
    export PATH="$(dirname "$(command -v cl)"):$PATH"
    cd src
    # /MT links the C runtime statically, so no VC++ redistributable is needed.
    MSYS_NO_PATHCONV=1 nmake -nologo -f Makefile.w32 gifsicle.exe \
      CFLAGS="-I.. -I..\\include -DHAVE_CONFIG_H -D_CONSOLE /W2 /O2 /MT"
    cd ..
    BIN="src/gifsicle.exe"
    EXT=".exe"
    ;;
esac

DEST="$OUT_DIR/gifsicle-${TRIPLE}${EXT}"
cp "$BIN" "$DEST"
case "$TRIPLE" in *-apple-darwin|*-linux-gnu) strip "$DEST" ;; esac
# A universal app is built as two per-arch cargo builds, and tauri-build checks
# the sidecar for each arch, so the fat binary is copied under both names too.
if [ "$TRIPLE" = universal-apple-darwin ]; then
  cp "$DEST" "$OUT_DIR/gifsicle-aarch64-apple-darwin"
  cp "$DEST" "$OUT_DIR/gifsicle-x86_64-apple-darwin"
fi
"$DEST" --version | head -1
echo "built $DEST"
