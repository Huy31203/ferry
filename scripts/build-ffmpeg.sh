#!/usr/bin/env bash
# Build the ffmpeg sidecar from pinned, verified sources: FFmpeg 9.0.2 with x264,
# libvpx, SVT-AV1, dav1d, Opus and LAME (plus VideoToolbox/AudioToolbox on macOS and
# NVENC/QSV/AMF on Windows). GPL-3.0-or-later build. There is no x265: HEVC is
# encoded by the OS or GPU hardware encoders only.
# Usage: scripts/build-ffmpeg.sh [target-triple]   (default: the host triple)
#        scripts/build-ffmpeg.sh --sources <dir>   (fetch every pinned source, build nothing)
# Output: src-tauri/binaries/ffmpeg-<triple>[.exe] and src-tauri/binaries/licenses/
set -euo pipefail

FFMPEG_URL=https://github.com/FFmpeg/FFmpeg.git
FFMPEG_TAG=n9.0.2
FFMPEG_COMMIT=946fcce07b6dcd0331c8cc609192aeff5e1924f8
X264_URL=https://code.videolan.org/videolan/x264.git
X264_COMMIT=31e19f92f00c7003fa115047ce50978bc98c3a0d
VPX_URL=https://github.com/webmproject/libvpx.git
VPX_TAG=v1.17.0
VPX_COMMIT=6df3ec34557879fff673706f4a1d9fbd0f3a6f0e
SVT_URL=https://gitlab.com/AOMediaCodec/SVT-AV1.git
SVT_TAG=v4.2.0
SVT_COMMIT=9292ec8e32bce26f781f277ec8739b53426c4300
DAV1D_URL=https://code.videolan.org/videolan/dav1d.git
DAV1D_TAG=1.5.4
DAV1D_COMMIT=54706fc6bc0cdecab7e9593974a4039cc038fca7
OPUS_URL=https://github.com/xiph/opus.git
OPUS_TAG=v1.6.1
OPUS_COMMIT=22244de5a79bd1d6d623c32e72bf1954b56235be
LAME_VERSION=3.100
LAME_SHA256=ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e
LAME_URL="https://downloads.sourceforge.net/project/lame/lame/${LAME_VERSION}/lame-${LAME_VERSION}.tar.gz"
ZLIB_VERSION=1.3.2
ZLIB_SHA256=bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16
ZLIB_URL="https://github.com/madler/zlib/releases/download/v${ZLIB_VERSION}/zlib-${ZLIB_VERSION}.tar.gz"
NVCODEC_URL=https://github.com/FFmpeg/nv-codec-headers.git
NVCODEC_TAG=n12.1.14.0
NVCODEC_COMMIT=1889e62e2d35ff7aa9baca2bceb14f053785e6f1
AMF_URL=https://github.com/GPUOpen-LibrariesAndSDKs/AMF.git
AMF_TAG=v1.5.2
AMF_COMMIT=eadd00804d5f7e5cd8c85d540073198312870776
VPL_URL=https://github.com/intel/libvpl.git
VPL_TAG=v2.17.0
VPL_COMMIT=d77f9195cf495b937631607333288fd917ae8939

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CACHE="$ROOT/target/ffmpeg-build"
SRC="$CACHE/src"
OUT_DIR="$ROOT/src-tauri/binaries"
LICENSES="$OUT_DIR/licenses"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# fetch_git <name> <url> <tag-or-empty> <commit> [sparse-path]
# Clones one pinned commit into $SRC/<name> and fails unless HEAD is exactly that commit.
fetch_git() {
  local name="$1" url="$2" tag="$3" commit="$4" sparse="${5:-}" dir="$SRC/$1"
  if [ -d "$dir/.git" ] && [ "$(git -C "$dir" rev-parse HEAD 2>/dev/null)" = "$commit" ]; then
    echo "reusing $name at $commit"
    return
  fi
  rm -rf "$dir" && mkdir -p "$dir"
  git -C "$dir" init -q
  git -C "$dir" remote add origin "$url"
  if [ -n "$sparse" ]; then
    git -C "$dir" config extensions.partialClone origin
    git -C "$dir" sparse-checkout set "$sparse"
  fi
  if [ -n "$tag" ]; then
    git -C "$dir" fetch -q --depth 1 ${sparse:+--filter=blob:none} origin "refs/tags/$tag:refs/tags/$tag"
    git -C "$dir" checkout -q "refs/tags/$tag"
  else
    git -C "$dir" fetch -q --depth 1 ${sparse:+--filter=blob:none} origin "$commit"
    git -C "$dir" checkout -q FETCH_HEAD
  fi
  local actual
  actual="$(git -C "$dir" rev-parse HEAD)"
  if [ "$actual" != "$commit" ]; then
    echo "$name: expected commit $commit, got $actual" >&2
    rm -rf "$dir"
    exit 1
  fi
}

fetch_lame() {
  local tarball="$CACHE/lame-${LAME_VERSION}.tar.gz" actual
  mkdir -p "$CACHE"
  [ -f "$tarball" ] || curl -fsSL "$LAME_URL" -o "$tarball"
  actual="$(sha256 "$tarball")"
  if [ "$actual" != "$LAME_SHA256" ]; then
    echo "LAME tarball checksum mismatch: got $actual" >&2
    rm -f "$tarball"
    exit 1
  fi
  if [ ! -f "$SRC/lame/include/libmp3lame.sym" ]; then
    rm -rf "$SRC/lame"
    mkdir -p "$SRC/lame"
    tar xzf "$tarball" -C "$SRC/lame" --strip-components=1
  fi
}

# Windows only: ffmpeg.exe links this zlib statically, so it is pinned and shipped as source.
fetch_zlib() {
  local tarball="$CACHE/zlib-${ZLIB_VERSION}.tar.gz" actual
  mkdir -p "$CACHE"
  [ -f "$tarball" ] || curl -fsSL "$ZLIB_URL" -o "$tarball"
  actual="$(sha256 "$tarball")"
  if [ "$actual" != "$ZLIB_SHA256" ]; then
    echo "zlib tarball checksum mismatch: got $actual" >&2
    rm -f "$tarball"
    exit 1
  fi
  if [ ! -d "$SRC/zlib" ]; then
    mkdir -p "$SRC/zlib"
    tar xzf "$tarball" -C "$SRC/zlib" --strip-components=1
  fi
}

fetch_common() {
  mkdir -p "$SRC"
  fetch_git ffmpeg "$FFMPEG_URL" "$FFMPEG_TAG" "$FFMPEG_COMMIT"
  fetch_git x264 "$X264_URL" "" "$X264_COMMIT"
  fetch_git libvpx "$VPX_URL" "$VPX_TAG" "$VPX_COMMIT"
  fetch_git svt-av1 "$SVT_URL" "$SVT_TAG" "$SVT_COMMIT"
  fetch_git dav1d "$DAV1D_URL" "$DAV1D_TAG" "$DAV1D_COMMIT"
  fetch_git opus "$OPUS_URL" "$OPUS_TAG" "$OPUS_COMMIT"
  fetch_lame
}

fetch_windows() {
  fetch_zlib
  fetch_git nv-codec-headers "$NVCODEC_URL" "$NVCODEC_TAG" "$NVCODEC_COMMIT"
  fetch_git amf "$AMF_URL" "$AMF_TAG" "$AMF_COMMIT" amf/public/include
  fetch_git libvpl "$VPL_URL" "$VPL_TAG" "$VPL_COMMIT"
}

# --sources <dir>: every pinned source for every platform, without .git.
if [ "${1:-}" = "--sources" ]; then
  DEST="${2:?usage: build-ffmpeg.sh --sources <dir>}"
  fetch_common
  fetch_windows
  mkdir -p "$DEST"
  for name in ffmpeg x264 libvpx svt-av1 dav1d opus nv-codec-headers libvpl; do
    mkdir -p "$DEST/$name"
    git -C "$SRC/$name" archive HEAD | tar -x -C "$DEST/$name"
  done
  # AMF is a sparse checkout, so copy the working tree.
  mkdir -p "$DEST/amf"
  tar -c --exclude=.git -C "$SRC/amf" . | tar -x -C "$DEST/amf"
  mkdir -p "$DEST/lame"
  cp -R "$SRC/lame/." "$DEST/lame/"
  mkdir -p "$DEST/zlib"
  cp -R "$SRC/zlib/." "$DEST/zlib/"
  echo "sources are in $DEST"
  exit 0
fi

TRIPLE="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
JOBS="$( (nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null) || echo 4)"

# Universal macOS: one native build per architecture (Rosetta for x86_64), then lipo.
if [ "$TRIPLE" = universal-apple-darwin ]; then
  if [ "$(uname -m)" != arm64 ]; then
    echo "universal-apple-darwin needs an Apple Silicon Mac (it builds x86_64 under Rosetta)" >&2
    exit 1
  fi
  arch -arm64 bash "$0" aarch64-apple-darwin
  arch -x86_64 bash "$0" x86_64-apple-darwin
  mkdir -p "$OUT_DIR"
  lipo -create "$CACHE/x86_64-apple-darwin/ffmpeg-bin" "$CACHE/aarch64-apple-darwin/ffmpeg-bin" \
    -output "$OUT_DIR/ffmpeg-universal-apple-darwin"
  if [ "$(lipo -archs "$OUT_DIR/ffmpeg-universal-apple-darwin")" != "x86_64 arm64" ]; then
    echo "the universal binary does not hold x86_64 and arm64" >&2
    exit 1
  fi
  cp "$OUT_DIR/ffmpeg-universal-apple-darwin" "$OUT_DIR/ffmpeg-aarch64-apple-darwin"
  cp "$OUT_DIR/ffmpeg-universal-apple-darwin" "$OUT_DIR/ffmpeg-x86_64-apple-darwin"
  echo "built $OUT_DIR/ffmpeg-universal-apple-darwin"
  exit 0
fi

EXT=""
WINDOWS=0
APPLE=0
case "$TRIPLE" in
  x86_64-apple-darwin|aarch64-apple-darwin)
    APPLE=1
    if [ "$TRIPLE" = x86_64-apple-darwin ]; then ARCH=x86_64; WANT=x86_64; VPX_TARGET=x86_64-darwin20-gcc
    else ARCH=arm64; WANT=arm64; VPX_TARGET=arm64-darwin20-gcc; fi
    if [ "$(uname -m)" != "$WANT" ]; then
      echo "$TRIPLE builds natively: run it on a $WANT shell (on Apple Silicon use 'arch -x86_64 bash $0 $TRIPLE')" >&2
      exit 1
    fi
    ;;
  x86_64-pc-windows-msvc)
    WINDOWS=1
    EXT=".exe"
    VPX_TARGET=x86_64-win64-gcc
    if [ "${MSYSTEM:-}" != MINGW64 ]; then
      echo "the Windows build runs in an MSYS2 MINGW64 shell (MSYSTEM=${MSYSTEM:-unset})" >&2
      exit 1
    fi
    ;;
  x86_64-unknown-linux-gnu)
    VPX_TARGET=x86_64-linux-gcc
    ;;
  *) echo "unsupported target: $TRIPLE" >&2; exit 1 ;;
esac

WORK="$CACHE/$TRIPLE"
PREFIX="$WORK/prefix"
mkdir -p "$PREFIX" "$OUT_DIR"
# Only the libraries built here are ever linked, never Homebrew's or MSYS2's.
export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig"
export PKG_CONFIG_PATH=""

MINVER=""
CMAKE_ARCH=()
MESON_CROSS=()
if [ "$APPLE" = 1 ]; then
  MINVER="-mmacosx-version-min=13.0"
  export CC="clang -arch $ARCH $MINVER"
  export CXX="clang++ -arch $ARCH $MINVER"
  export CFLAGS="-arch $ARCH $MINVER"
  export CXXFLAGS="-arch $ARCH $MINVER"
  export LDFLAGS="-arch $ARCH $MINVER"
  CMAKE_ARCH=(-DCMAKE_OSX_ARCHITECTURES="$ARCH" -DCMAKE_OSX_DEPLOYMENT_TARGET=13.0)
  # Homebrew's meson is arm64-only and runs natively even from the Rosetta shell, so it
  # must be told the target CPU or it configures dav1d for arm64.
  MESON_CPU=aarch64
  [ "$ARCH" = x86_64 ] && MESON_CPU=x86_64
  cat >"$WORK/meson-cross.ini" <<EOT
[binaries]
c = ['clang', '-arch', '$ARCH', '$MINVER']
nasm = 'nasm'

[host_machine]
system = 'darwin'
cpu_family = '$MESON_CPU'
cpu = '$MESON_CPU'
endian = 'little'
EOT
  MESON_CROSS=(--cross-file "$WORK/meson-cross.ini")
fi

fetch_common
[ "$WINDOWS" = 1 ] && fetch_windows

# Each library builds in a fresh copy so a rebuild never sees stale objects.
fresh() { rm -rf "$WORK/$1" && cp -R "$SRC/$1" "$WORK/$1" && cd "$WORK/$1"; }

echo "== x264"
fresh x264
./configure --prefix="$PREFIX" --enable-static --disable-cli --enable-pic --disable-opencl >/dev/null
make -j"$JOBS" >/dev/null && make install >/dev/null

echo "== libvpx"
fresh libvpx
./configure --prefix="$PREFIX" --target="$VPX_TARGET" --enable-static --disable-shared --enable-pic \
  --enable-vp9-highbitdepth --disable-examples --disable-tools --disable-docs --disable-unit-tests >/dev/null
make -j"$JOBS" >/dev/null && make install >/dev/null

echo "== SVT-AV1"
fresh svt-av1
cmake -G Ninja -B build -DBUILD_SHARED_LIBS=OFF -DBUILD_APPS=OFF -DBUILD_DEC=OFF -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$PREFIX" "${CMAKE_ARCH[@]}" >/dev/null
cmake --build build --target install >/dev/null

echo "== dav1d"
fresh dav1d
meson setup build "${MESON_CROSS[@]}" --prefix="$PREFIX" --libdir=lib --default-library=static --buildtype=release \
  -Denable_tools=false -Denable_tests=false
ninja -C build install >/dev/null

echo "== Opus"
fresh opus
cmake -G Ninja -B build -DOPUS_BUILD_SHARED_LIBRARY=OFF -DOPUS_BUILD_TESTING=OFF -DOPUS_BUILD_PROGRAMS=OFF \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$PREFIX" "${CMAKE_ARCH[@]}" >/dev/null
cmake --build build --target install >/dev/null

echo "== LAME"
fresh lame
# The known macOS link fix: lame_init_old is listed but not defined.
[ -f include/libmp3lame.sym ] || { echo "LAME: include/libmp3lame.sym is missing in $PWD" >&2; ls -la . include >&2; exit 1; }
sed -i.bak '/lame_init_old/d' include/libmp3lame.sym
LAME_NASM=""
[ "${TRIPLE%%-*}" = x86_64 ] && LAME_NASM="--enable-nasm"
./configure --prefix="$PREFIX" --enable-static --disable-shared --disable-frontend --disable-decoder $LAME_NASM >/dev/null
make -j"$JOBS" >/dev/null && make install >/dev/null

FFMPEG_EXTRA=()
if [ "$WINDOWS" = 1 ]; then
  echo "== zlib"
  fresh zlib
  ./configure --static --prefix="$PREFIX" >/dev/null
  make -j"$JOBS" >/dev/null && make install >/dev/null
  echo "== nv-codec-headers, AMF headers, libvpl"
  fresh nv-codec-headers
  make install PREFIX="$PREFIX" >/dev/null
  fresh amf
  mkdir -p "$PREFIX/include/AMF" && cp -R amf/public/include/* "$PREFIX/include/AMF/"
  fresh libvpl
  # libvpl's pre-VS2005 fallback (_MSC_VER < 1400) also fires under MinGW and breaks wcscpy_s.
  sed -i 's/^#if _MSC_VER < 1400/#if defined(_MSC_VER) \&\& _MSC_VER < 1400/' libvpl/src/windows/mfx_dispatcher_defs.h
  cmake -G Ninja -B build -DBUILD_SHARED_LIBS=OFF -DBUILD_TESTS=OFF -DBUILD_EXAMPLES=OFF -DINSTALL_EXAMPLES=OFF \
    -DBUILD_TOOLS=OFF -DCMAKE_INSTALL_PREFIX="$PREFIX" >/dev/null
  cmake --build build --target install
  FFMPEG_EXTRA=(--enable-ffnvcodec --enable-nvenc --enable-amf --enable-libvpl --extra-ldflags=-static --extra-libs=-lstdc++)
elif [ "$APPLE" = 1 ]; then
  FFMPEG_EXTRA=(--enable-videotoolbox --enable-audiotoolbox --cc="clang -arch $ARCH" --arch="$ARCH")
fi

echo "== FFmpeg"
fresh ffmpeg
./configure --prefix="$PREFIX" --pkg-config-flags=--static \
  --extra-cflags="-I$PREFIX/include $MINVER" --extra-ldflags="-L$PREFIX/lib $MINVER" \
  --enable-gpl --enable-version3 --disable-autodetect --disable-shared --enable-static \
  --disable-doc --disable-debug --enable-stripping --disable-ffplay --disable-ffprobe --disable-network \
  --enable-zlib --enable-libx264 --enable-libvpx --enable-libsvtav1 --enable-libdav1d --enable-libopus \
  --enable-libmp3lame "${FFMPEG_EXTRA[@]}" >"$WORK/ffmpeg-configure.out" 2>&1 || {
  echo "ffmpeg configure failed" >&2
  tail -n 40 "$WORK/ffmpeg-configure.out" >&2
  echo "== ffbuild/config.log (tail)" >&2
  tail -n 80 ffbuild/config.log >&2 || true
  exit 1
}
make -j"$JOBS" >/dev/null
BIN="$WORK/ffmpeg/ffmpeg$EXT"
strip "$BIN"
cp "$BIN" "$WORK/ffmpeg-bin$EXT"
BIN="$WORK/ffmpeg-bin$EXT"

echo "== checks"
CONF="$("$BIN" -hide_banner -buildconf)"
for want in --enable-gpl --enable-version3; do
  grep -q -- "$want" <<<"$CONF" || { echo "the build lacks $want" >&2; exit 1; }
done
for never in --enable-nonfree --enable-libx265; do
  if grep -q -- "$never" <<<"$CONF"; then echo "the build must not have $never" >&2; exit 1; fi
done
if [ "$APPLE" = 1 ]; then
  ENCODERS="$("$BIN" -hide_banner -encoders)"
  for want in aac_at h264_videotoolbox hevc_videotoolbox; do
    grep -qE " $want " <<<"$ENCODERS" || { echo "the build lacks the $want encoder (--disable-autodetect needs --enable-videotoolbox --enable-audiotoolbox)" >&2; exit 1; }
  done
  if otool -L "$BIN" | tail -n +2 | awk '{print $1}' | grep -vE '^(/usr/lib/|/System/Library/)'; then
    echo "the binary links something outside /usr/lib and /System/Library" >&2
    exit 1
  fi
fi
if [ "$WINDOWS" = 1 ]; then
  if objdump -p "$BIN" | grep 'DLL Name' | grep -iE 'libwinpthread-1.dll|libstdc\+\+-6.dll|libgcc_s_seh-1.dll|zlib1.dll'; then
    echo "ffmpeg.exe depends on a MinGW runtime DLL" >&2
    exit 1
  fi
fi

echo "== licences"
lic() { mkdir -p "$LICENSES/$1" && cp "$SRC/$2" "$LICENSES/$1/"; }
rm -rf "$LICENSES/ffmpeg" "$LICENSES/x264" "$LICENSES/libvpx" "$LICENSES/svt-av1" "$LICENSES/dav1d" \
  "$LICENSES/opus" "$LICENSES/lame" "$LICENSES/libvpl" "$LICENSES/amf" "$LICENSES/nv-codec-headers" \
  "$LICENSES/zlib" "$LICENSES/mingw-w64"
lic ffmpeg ffmpeg/COPYING.GPLv3
lic ffmpeg ffmpeg/LICENSE.md
lic x264 x264/COPYING
lic libvpx libvpx/LICENSE
lic libvpx libvpx/PATENTS
lic svt-av1 svt-av1/LICENSE.md
lic svt-av1 svt-av1/PATENTS.md
lic dav1d dav1d/COPYING
lic opus opus/COPYING
lic lame lame/COPYING
if [ "$WINDOWS" = 1 ]; then
  lic libvpl libvpl/LICENSE
  lic amf amf/LICENSE.txt
  lic zlib zlib/LICENSE
  # winpthreads and the GCC runtime are linked statically (-static) from the MSYS2 toolchain; record which.
  mkdir -p "$LICENSES/mingw-w64"
  pacman -Q mingw-w64-x86_64-gcc mingw-w64-x86_64-winpthreads mingw-w64-x86_64-crt-git > "$LICENSES/mingw-w64/PACKAGES.txt" 2>&1 || true
  find /mingw64/share/licenses -ipath '*winpthreads*' -type f -exec cp {} "$LICENSES/mingw-w64/" \; 2>/dev/null || true
  mkdir -p "$LICENSES/nv-codec-headers"
  sed -n '1,/\*\//p' "$SRC/nv-codec-headers/include/ffnvcodec/nvEncodeAPI.h" > "$LICENSES/nv-codec-headers/LICENSE"
fi
cp "$ROOT/THIRD-PARTY-NOTICES.md" "$LICENSES/THIRD-PARTY-NOTICES.md"

DEST="$OUT_DIR/ffmpeg-${TRIPLE}${EXT}"
cp "$BIN" "$DEST"
"$DEST" -version | head -1
echo "size: $(du -m "$DEST" | cut -f1) MB"
echo "built $DEST"
