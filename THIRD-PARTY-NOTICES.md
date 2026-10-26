# Third-party notices

Ferry is GPL-3.0-or-later. It ships two helper programs, `gifsicle` and `ffmpeg`, next to the app. Their licence files are
in the bundle under `licenses/<component>/`. The licences of the Rust crates linked into Ferry itself are in
`licenses/rust-crates-NOTICES.html`, and those of PDFium in `pdfium-NOTICE.txt`; both are described below.

The ffmpeg helper is built by `scripts/build-ffmpeg.sh` with `--enable-gpl --enable-version3`, so it is
**GPL-3.0-or-later**. It is never built with `--enable-nonfree`. It contains **no x265**: HEVC is encoded only by the
operating system's or the graphics card's hardware encoders (VideoToolbox, NVENC, Quick Sync, AMF).

The corresponding source of every component below, the build scripts and this file are attached to each GitHub Release as
`ferry-<tag>-third-party-sources.tar.xz`.

This software is based in part on the work of the Independent JPEG Group. (FFmpeg's `jfdctfst.c`, `jfdctint_template.c`
and `jrevdct.c`, unchanged.)

## gifsicle 1.98

- Source: https://www.lcdf.org/gifsicle/gifsicle-1.98.tar.gz, SHA-256 `e9d4cc0816db95c64d32e79f778836db0f1f7eb15868490bd330104e5146b66e`
- Licence: GPL-2.0-or-later. File in the bundle: `licenses/gifsicle/COPYING`

## FFmpeg 9.0.2

- Source: https://github.com/FFmpeg/FFmpeg.git, tag `n9.0.2`, commit `946fcce07b6dcd0331c8cc609192aeff5e1924f8`
- Licence: GPL-3.0-or-later as configured (the sources are LGPL-2.1-or-later and GPL-2.0-or-later). Files in the bundle: `licenses/ffmpeg/COPYING.GPLv3`, `licenses/ffmpeg/LICENSE.md`

## x264 (stable)

- Source: https://code.videolan.org/videolan/x264.git, commit `31e19f92f00c7003fa115047ce50978bc98c3a0d`
- Licence: GPL-2.0-or-later. File in the bundle: `licenses/x264/COPYING`

## libvpx 1.17.0

- Source: https://github.com/webmproject/libvpx.git, tag `v1.17.0`, commit `6df3ec34557879fff673706f4a1d9fbd0f3a6f0e`
- Licence: BSD-3-Clause with Google's patent grant. Files in the bundle: `licenses/libvpx/LICENSE`, `licenses/libvpx/PATENTS`

## SVT-AV1 4.2.0

- Source: https://gitlab.com/AOMediaCodec/SVT-AV1.git, tag `v4.2.0`, commit `9292ec8e32bce26f781f277ec8739b53426c4300`
- Licence: BSD-3-Clause-Clear with the AOM Patent License 1.0. Files in the bundle: `licenses/svt-av1/LICENSE.md`, `licenses/svt-av1/PATENTS.md`

## dav1d 1.5.4

- Source: https://code.videolan.org/videolan/dav1d.git, tag `1.5.4`, commit `54706fc6bc0cdecab7e9593974a4039cc038fca7`
- Licence: BSD-2-Clause. File in the bundle: `licenses/dav1d/COPYING`

## Opus 1.6.1

- Source: https://github.com/xiph/opus.git, tag `v1.6.1`, commit `22244de5a79bd1d6d623c32e72bf1954b56235be`
- Licence: BSD-3-Clause. File in the bundle: `licenses/opus/COPYING`

## LAME 3.100

- Source: https://downloads.sourceforge.net/project/lame/lame/3.100/lame-3.100.tar.gz, SHA-256 `ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e`
- Licence: LGPL-2.0-or-later. File in the bundle: `licenses/lame/COPYING`

## Windows only

- nv-codec-headers 12.1.14.0 (https://github.com/FFmpeg/nv-codec-headers.git, tag `n12.1.14.0`, commit `1889e62e2d35ff7aa9baca2bceb14f053785e6f1`): MIT. File in the bundle: `licenses/nv-codec-headers/LICENSE`
- AMF headers 1.5.2 (https://github.com/GPUOpen-LibrariesAndSDKs/AMF.git, tag `v1.5.2`, commit `eadd00804d5f7e5cd8c85d540073198312870776`): MIT. File in the bundle: `licenses/amf/LICENSE.txt`
- libvpl 2.17.0 (https://github.com/intel/libvpl.git, tag `v2.17.0`, commit `d77f9195cf495b937631607333288fd917ae8939`): MIT. File in the bundle: `licenses/libvpl/LICENSE`
- zlib 1.3.2 (https://github.com/madler/zlib/releases/download/v1.3.2/zlib-1.3.2.tar.gz, SHA-256 `bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16`): zlib licence, linked statically into `ffmpeg.exe`. File in the bundle: `licenses/zlib/LICENSE`
- winpthreads and the MinGW-w64 C runtime, linked statically into `ffmpeg.exe` from the MSYS2 MINGW64 toolchain (`-static`). winpthreads is under permissive licences (the mingw-w64 project's MIT-style licence; some files carry BSD-style or ZPL-2.1 notices, see `mingw-w64-libraries/winpthreads/COPYING`); the C runtime is public domain / ZPL-2.1. Source: https://github.com/mingw-w64/mingw-w64 (`mingw-w64-libraries/winpthreads`), at the release shipped in the MSYS2 package `mingw-w64-x86_64-winpthreads` used for the build; the package names and versions are recorded in `licenses/mingw-w64/PACKAGES.txt`, with the licence files MSYS2 ships for winpthreads alongside it.
- GCC runtime libraries (libgcc, static): GPL-3.0-or-later **with the GCC Runtime Library Exception 3.1**, https://gcc.gnu.org/onlinedocs/libstdc++/manual/license.html. The exception lets the combined `ffmpeg.exe` be conveyed under GPL-3.0-or-later. Source: the GCC release shipped in the MSYS2 package `mingw-w64-x86_64-gcc` (version in `licenses/mingw-w64/PACKAGES.txt`; https://gcc.gnu.org/git.html).

## PDFium (chromium/7881)

- Source: https://pdfium.googlesource.com/pdfium (branch `chromium/7881`), built unmodified by https://github.com/bblanchon/pdfium-binaries (release `chromium/7881`). `scripts/fetch-pdfium.sh` downloads the pinned archives and checks their SHA-256.
- Licence: Apache-2.0, plus the BSD-style and other permissive licences of the libraries PDFium bundles (for example FreeType, ICU, lcms2, libjpeg-turbo, libpng, libtiff, OpenJPEG, zlib, Abseil, agg). Their texts are collected, with PDFium's own `LICENSE`, in `pdfium-NOTICE.txt`, which Ferry ships as a resource next to the app.
- PDFium is a separate shared library (`libpdfium.dylib` in the macOS app's Frameworks folder, `pdfium.dll` next to `Ferry.exe`). Ferry loads it at run time through the `pdfium-render` crate and does not link it statically.

## Rust crates

Ferry links open-source Rust crates from crates.io (see `Cargo.lock` for the exact versions). Almost all are MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unicode-3.0 or similar permissive licences, which require that their copyright and licence notices travel with the binary. The release workflow generates them with `cargo about` (`about.toml`, `about.hbs`) into `licenses/rust-crates-NOTICES.html` in the bundle. It lists every crate, version, licence and licence text. Among them are `rav1d`, `ssimulacra2` and `avif-decode` (BSD), `lopdf`, `tiff` and `pdfium-render` (MIT or Apache-2.0), `moxcms` (BSD-3-Clause or Apache-2.0) and `mozjpeg` (IJG, Zlib, BSD-3-Clause). `imagequant` is GPL-3.0-or-later, the same licence as Ferry.

The following crates are under the **Mozilla Public License 2.0**: `avif-parse` (used for AVIF decoding), and `cssparser`, `cssparser-macros`, `dtoa-short`, `option-ext` and `selectors` (pulled in transitively, through `dom_query` and `dirs-sys`). The MPL-2.0 is a file-level copyleft licence: Ferry does not modify these crates, and uses them unmodified as published. Their complete source is available from crates.io at the version pinned in `Cargo.lock` (for example https://crates.io/crates/avif-parse, https://github.com/kornelski/avif-parse) and in their upstream repositories. The licence text is in `licenses/rust-crates-NOTICES.html`, and the Mozilla Public License 2.0 is at https://www.mozilla.org/MPL/2.0/. Ferry's own code stays under GPL-3.0-or-later, which MPL-2.0 section 3.3 permits as a combined work.

## System components (not shipped)

macOS: VideoToolbox and AudioToolbox frameworks (hardware H.264 and HEVC encoders, `aac_at`). Windows: the NVIDIA, Intel
and AMD drivers provide the NVENC, Quick Sync and AMF runtimes.

## Patents

H.264, HEVC and AAC are covered by patent pools. Ferry ships a software H.264 encoder (x264) and the native AAC encoder; HEVC
is encoded only by hardware encoders. VP9 and AV1 come with royalty-free grants from Google and the Alliance for Open Media;
MP3 patents have expired. Seek legal advice before a 1.0 release.
