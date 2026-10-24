# Third-party notices

Ferry is GPL-3.0-or-later. It ships two helper programs, `gifsicle` and `ffmpeg`, next to the app. Their licence files are
in the bundle under `licenses/<component>/`.

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

## System components (not shipped)

macOS: VideoToolbox and AudioToolbox frameworks (hardware H.264 and HEVC encoders, `aac_at`). Windows: the NVIDIA, Intel
and AMD drivers provide the NVENC, Quick Sync and AMF runtimes. Zlib is linked statically on Windows (zlib licence).

## Patents

H.264, HEVC and AAC are covered by patent pools. Ferry ships a software H.264 encoder (x264) and the native AAC encoder; HEVC
is encoded only by hardware encoders. VP9 and AV1 come with royalty-free grants from Google and the Alliance for Open Media;
MP3 patents have expired. Seek legal advice before a 1.0 release.
