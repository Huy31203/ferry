//! Deterministic synthetic images for tests, so no binary fixtures live in git.

/// A screenshot-like RGBA image: flat panels, a gradient bar and rows of dark
/// "text" speckles. More than 256 colours, but low entropy, like a real UI.
pub fn screenshot_rgba(w: u32, h: u32) -> Vec<u8> {
    let mut seed = 0x2545_F491_u32;
    let mut noise = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 24) as u8
    };
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (mut r, mut g, mut b) = if x < w / 5 {
                (233, 233, 237) // sidebar
            } else if y < h / 8 {
                // gradient header
                let t = (x * 255 / w) as u8;
                (t, 100 + t / 3, 240 - t / 2)
            } else {
                (250, 250, 252) // content
            };
            if x >= w / 5 && y > h / 8 && (y / 6) % 3 == 0 && noise() > 150 {
                (r, g, b) = (40 + noise() % 30, 40 + noise() % 30, 48 + noise() % 30);
            }
            px.extend_from_slice(&[r, g, b, 255]);
        }
    }
    px
}

/// A plain (unoptimised) RGBA PNG of `screenshot_rgba`.
pub fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    writer.write_image_data(&screenshot_rgba(w, h)).unwrap();
    writer.finish().unwrap();
    out
}

/// A valid small PNG whose header claims `w`×`h` pixels, far more than its data holds.
pub fn png_claiming_size(w: u32, h: u32) -> Vec<u8> {
    let mut out = png(1, 1);
    out[16..20].copy_from_slice(&w.to_be_bytes());
    out[20..24].copy_from_slice(&h.to_be_bytes());
    let crc = crc32(&out[12..29]);
    out[29..33].copy_from_slice(&crc.to_be_bytes());
    out
}

/// Insert an ancillary chunk right after IHDR.
pub fn with_png_chunk(png: &[u8], name: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let ihdr_end = 8 + 12 + 13;
    let mut out = png[..ihdr_end].to_vec();
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = name.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
    out.extend_from_slice(&png[ihdr_end..]);
    out
}

/// The names of every chunk in a PNG, in file order.
pub fn png_chunk_names(png: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut pos = 8;
    while let Some(head) = png.get(pos..pos + 8) {
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        names.push(String::from_utf8_lossy(&head[4..8]).into_owned());
        pos += 12 + len;
    }
    names
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

/// A GIF with `frames` frames of `screenshot_rgba`-like content, 64×48, stored
/// without LZW compression (a clear code every 200 pixels keeps codes 9 bits).
pub fn gif(frames: usize) -> Vec<u8> {
    let (w, h) = (64_u16, 48_u16);
    let mut out = b"GIF89a".to_vec();
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&[0xF7, 0, 0]); // global table, 256 colours
    for i in 0..256_u32 {
        out.extend_from_slice(&[i as u8, (i * 7) as u8, (255 - i) as u8]);
    }
    if frames > 1 {
        out.extend_from_slice(b"\x21\xFF\x0BNETSCAPE2.0\x03\x01\x00\x00\x00"); // loop forever
    }
    for f in 0..frames {
        out.extend_from_slice(&[0x21, 0xF9, 4, 0, 10, 0, 0, 0]); // 100 ms per frame
        out.push(0x2C);
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&h.to_le_bytes());
        out.push(0);
        out.push(8); // LZW minimum code size
        let pixels: Vec<u8> = (0..u32::from(w) * u32::from(h))
            .map(|i| ((i / 24 + f as u32 * 40) % 256) as u8)
            .collect();
        for block in lzw_stored(&pixels).chunks(255) {
            out.push(block.len() as u8);
            out.extend_from_slice(block);
        }
        out.push(0);
    }
    out.push(0x3B);
    out
}

/// GIF LZW with min code size 8 that never compresses: 9-bit codes, a clear
/// code (256) before every 200 pixels, end code (257) last.
fn lzw_stored(pixels: &[u8]) -> Vec<u8> {
    struct Bits {
        out: Vec<u8>,
        acc: u32,
        n: u32,
    }
    impl Bits {
        fn put(&mut self, code: u32) {
            self.acc |= code << self.n;
            self.n += 9;
            while self.n >= 8 {
                self.out.push(self.acc as u8);
                self.acc >>= 8;
                self.n -= 8;
            }
        }
    }
    let mut bits = Bits {
        out: Vec::new(),
        acc: 0,
        n: 0,
    };
    for run in pixels.chunks(200) {
        bits.put(256);
        for &p in run {
            bits.put(u32::from(p));
        }
    }
    bits.put(257);
    if bits.n > 0 {
        bits.out.push(bits.acc as u8);
    }
    bits.out
}

/// A minimal RIFF/WebP container with the animation flag and an ANIM chunk.
pub fn animated_webp() -> Vec<u8> {
    let mut body = b"WEBP".to_vec();
    body.extend_from_slice(b"VP8X");
    body.extend_from_slice(&10_u32.to_le_bytes());
    body.extend_from_slice(&[0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    body.extend_from_slice(b"ANIM");
    body.extend_from_slice(&6_u32.to_le_bytes());
    body.extend_from_slice(&[0; 6]);
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Extra segments for `jpeg`.
#[derive(Default)]
pub struct JpegExtras {
    /// A complete APP1 payload (`Exif\0\0` + TIFF).
    pub exif: Option<Vec<u8>>,
    /// A complete APP2 payload (`ICC_PROFILE\0` + sequence bytes + profile).
    pub icc: Option<Vec<u8>>,
    pub dpi: Option<u16>,
}

/// A baseline RGB JPEG of `screenshot_rgba`.
pub fn jpeg(w: u32, h: u32, quality: u8, extras: &JpegExtras) -> Vec<u8> {
    let rgb: Vec<u8> = screenshot_rgba(w, h)
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let mut enc = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    enc.set_size(w as usize, h as usize);
    enc.set_quality(f32::from(quality));
    if let Some(dpi) = extras.dpi {
        enc.set_pixel_density(mozjpeg::PixelDensity {
            unit: mozjpeg::PixelDensityUnit::Inches,
            x: dpi,
            y: dpi,
        });
    }
    let mut enc = enc.start_compress(Vec::new()).unwrap();
    if let Some(icc) = &extras.icc {
        enc.write_marker(mozjpeg::Marker::APP(2), icc);
    }
    if let Some(exif) = &extras.exif {
        enc.write_marker(mozjpeg::Marker::APP(1), exif);
    }
    enc.write_scanlines(&rgb).unwrap();
    enc.finish().unwrap()
}

/// A grayscale JPEG of the red channel of `screenshot_rgba`.
pub fn jpeg_gray(w: u32, h: u32, quality: u8) -> Vec<u8> {
    let gray: Vec<u8> = screenshot_rgba(w, h)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[0])
        .collect();
    let mut enc = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_GRAYSCALE);
    enc.set_size(w as usize, h as usize);
    enc.set_quality(f32::from(quality));
    let mut enc = enc.start_compress(Vec::new()).unwrap();
    enc.write_scanlines(&gray).unwrap();
    enc.finish().unwrap()
}

/// A little-endian EXIF APP1 payload: an Orientation entry plus `padding`
/// bytes of junk, standing in for a camera's large EXIF block.
pub fn exif_with_orientation(orientation: u16, padding: usize) -> Vec<u8> {
    let mut out = b"Exif\0\0II\x2A\0\x08\0\0\0".to_vec();
    out.extend_from_slice(&[1, 0]); // one entry
    out.extend_from_slice(&[0x12, 0x01, 3, 0, 1, 0, 0, 0]);
    out.extend_from_slice(&orientation.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    out.extend(std::iter::repeat_n(0xAB, padding));
    out
}

/// A lossy WebP of `screenshot_rgba`, wrapped with `extras` when given.
pub fn webp(w: u32, h: u32, quality: u8, extras: &crate::optimise::webp::Extras) -> Vec<u8> {
    let rgba = screenshot_rgba(w, h);
    let encoded = webp::Encoder::from_rgba(&rgba, w, h).encode(f32::from(quality));
    crate::optimise::webp::assemble(&encoded, w, h, false, extras)
}

/// A lossy WebP whose top-left quadrant is fully transparent.
pub fn webp_with_transparent_corner(w: u32, h: u32) -> Vec<u8> {
    let mut rgba = screenshot_rgba(w, h);
    for y in 0..h / 2 {
        for x in 0..w / 2 {
            rgba[((y * w + x) * 4 + 3) as usize] = 0;
        }
    }
    webp::Encoder::from_rgba(&rgba, w, h).encode(95.0).to_vec()
}

/// `FERRY_GIFSICLE` (the sidecar's path), when it is set and exists. GIF tests
/// skip without it; CI sets it and fails if it is missing.
pub fn gifsicle() -> Option<std::path::PathBuf> {
    std::env::var_os("FERRY_GIFSICLE")
        .map(std::path::PathBuf::from)
        .filter(|path| path.exists())
}

/// `FERRY_FFMPEG` (the sidecar's path), when it is set and exists. Video tests
/// skip without it; CI sets it and fails if it is missing.
pub fn ffmpeg() -> Option<std::path::PathBuf> {
    std::env::var_os(crate::ffmpeg::ENV)
        .map(std::path::PathBuf::from)
        .filter(|path| path.exists())
}

/// What `video` makes.
pub struct VideoSpec {
    pub size: (u32, u32),
    pub secs: u32,
    pub fps: u32,
    /// "libx264" or "libvpx-vp9" (the build has no libx265).
    pub codec: &'static str,
    pub audio_tracks: u32,
}

/// A synthetic clip in `dir` (lavfi testsrc2 plus one sine per audio track), or
/// `None` without FERRY_FFMPEG.
pub fn video(dir: &std::path::Path, file: &str, spec: &VideoSpec) -> Option<std::path::PathBuf> {
    ffmpeg()?;
    let path = dir.join(file);
    let mut args: Vec<std::ffi::OsString> = Vec::new();
    let mut add = |items: &[&str]| args.extend(items.iter().map(std::ffi::OsString::from));
    let (w, h) = spec.size;
    add(&["-f", "lavfi", "-i"]);
    add(&[&format!(
        "testsrc2=size={w}x{h}:rate={}:duration={}",
        spec.fps, spec.secs
    )]);
    for k in 1..=spec.audio_tracks {
        add(&["-f", "lavfi", "-i"]);
        add(&[&format!(
            "sine=frequency={}:duration={}",
            440 * k,
            spec.secs
        )]);
    }
    add(&["-map", "0:v"]);
    for k in 1..=spec.audio_tracks {
        add(&["-map", &format!("{k}:a")]);
    }
    if spec.codec == "libvpx-vp9" {
        add(&[
            "-c:v",
            "libvpx-vp9",
            "-deadline",
            "realtime",
            "-cpu-used",
            "8",
        ]);
        add(&["-crf", "10", "-b:v", "0"]);
    } else {
        add(&["-c:v", spec.codec, "-preset", "ultrafast", "-crf", "10"]);
    }
    add(&["-pix_fmt", "yuv420p"]);
    let webm = path.extension().is_some_and(|e| e == "webm");
    add(&["-c:a", if webm { "libopus" } else { "aac" }, "-shortest"]);
    args.push(path.clone().into());
    crate::ffmpeg::run(&args, &crate::cancel::CancelToken::new(), &mut |_| {}).ok()?;
    Some(path)
}

/// Insert a comment extension right after the global colour table.
pub fn with_gif_comment(gif: &[u8], text: &str) -> Vec<u8> {
    let at = 13 + 768; // header + screen descriptor + 256-colour table
    let mut out = gif[..at].to_vec();
    out.extend_from_slice(&[0x21, 0xFE, text.len() as u8]);
    out.extend_from_slice(text.as_bytes());
    out.push(0);
    out.extend_from_slice(&gif[at..]);
    out
}

/// Set an extended attribute (macOS tests).
#[cfg(target_os = "macos")]
pub fn set_xattr(path: &std::path::Path, name: &std::ffi::CStr, value: &[u8]) {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    let rc = unsafe {
        libc::setxattr(
            path.as_ptr(),
            name.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
            0,
        )
    };
    assert_eq!(rc, 0, "{}", std::io::Error::last_os_error());
}

#[cfg(target_os = "macos")]
pub fn get_xattr(path: &std::path::Path, name: &std::ffi::CStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    let mut buf = vec![0u8; 256];
    let len = unsafe {
        libc::getxattr(
            path.as_ptr(),
            name.as_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
            0,
            0,
        )
    };
    assert!(len >= 0, "{}", std::io::Error::last_os_error());
    buf.truncate(len as usize);
    buf
}

// ---- sE pdf-formats fixtures ----

/// An ISO box: size, type, payload.
fn boxed(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = (payload.len() as u32 + 8).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    out
}

/// A `ftyp` box with a major brand and compatible brands.
pub fn ftyp(major: &[u8; 4], compatible: &[&[u8; 4]]) -> Vec<u8> {
    let mut payload = major.to_vec();
    payload.extend_from_slice(&[0; 4]);
    payload.extend(compatible.iter().flat_map(|b| b.iter().copied()));
    boxed(b"ftyp", &payload)
}

/// `ftyp` + `meta` → `iprp` → `ipco` holding one `ispe` per size and an optional `irot`.
pub fn heif(major: &[u8; 4], sizes: &[(u32, u32)], irot: Option<u8>) -> Vec<u8> {
    let mut ipco = Vec::new();
    for &(w, h) in sizes {
        let mut ispe = vec![0; 4];
        ispe.extend_from_slice(&w.to_be_bytes());
        ispe.extend_from_slice(&h.to_be_bytes());
        ipco.extend(boxed(b"ispe", &ispe));
    }
    if let Some(angle) = irot {
        ipco.extend(boxed(b"irot", &[angle]));
    }
    let mut meta = vec![0; 4]; // version and flags
    meta.extend(boxed(b"iprp", &boxed(b"ipco", &ipco)));
    let mut out = ftyp(major, &[b"mif1"]);
    out.extend(boxed(b"meta", &meta));
    out
}

/// A little-endian TIFF header and one IFD per page, holding tags 256 and 257
/// (LONG) and `extra_tag` (SHORT 1). No pixel data.
pub fn tiff_ifds(pages: &[(u32, u32)], extra_tag: Option<u16>) -> Vec<u8> {
    let entries = 2 + usize::from(extra_tag.is_some());
    let ifd_len = 2 + 12 * entries + 4;
    let mut out = b"II*\0".to_vec();
    out.extend_from_slice(&8_u32.to_le_bytes());
    for (i, &(w, h)) in pages.iter().enumerate() {
        out.extend_from_slice(&(entries as u16).to_le_bytes());
        for (tag, kind, value) in [(256_u16, 4_u16, w), (257, 4, h)] {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&1_u32.to_le_bytes());
            out.extend_from_slice(&value.to_le_bytes());
        }
        if let Some(tag) = extra_tag {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&3_u16.to_le_bytes());
            out.extend_from_slice(&1_u32.to_le_bytes());
            out.extend_from_slice(&[1, 0, 0, 0]);
        }
        let next = if i + 1 < pages.len() {
            8 + ifd_len * (i + 1)
        } else {
            0
        };
        out.extend_from_slice(&(next as u32).to_le_bytes());
    }
    out
}

/// The colour layouts `tiff` can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TiffColour {
    Rgb8,
    Rgba8,
    Gray8,
    Gray16,
    Cmyk8,
}

/// One page of a `tiff` fixture.
#[derive(Debug, Clone)]
pub struct TiffPage {
    pub size: (u32, u32),
    pub colour: TiffColour,
    pub orientation: u16,
    pub icc: Option<Vec<u8>>,
}

impl Default for TiffPage {
    fn default() -> Self {
        Self {
            size: (64, 48),
            colour: TiffColour::Rgb8,
            orientation: 1,
            icc: None,
        }
    }
}

/// An uncompressed TIFF with one IFD per page, from `screenshot_rgba` pixels.
pub fn tiff(pages: &[TiffPage]) -> Vec<u8> {
    use tiff::encoder::{TiffEncoder, colortype};
    let mut out = std::io::Cursor::new(Vec::new());
    let mut enc = TiffEncoder::new(&mut out).unwrap();
    for page in pages {
        let (w, h) = page.size;
        let rgba = screenshot_rgba(w, h);
        let px = rgba.as_chunks::<4>().0;
        macro_rules! write {
            ($colour:ty, $data:expr) => {{
                let mut image = enc.new_image::<$colour>(w, h).unwrap();
                if page.colour == TiffColour::Rgba8 {
                    let extra = [tiff::tags::ExtraSamples::UnassociatedAlpha.to_u16()];
                    image
                        .encoder()
                        .write_tag(tiff::tags::Tag::ExtraSamples, &extra[..])
                        .unwrap();
                }
                image
                    .encoder()
                    .write_tag(tiff::tags::Tag::Orientation, page.orientation)
                    .unwrap();
                if let Some(icc) = &page.icc {
                    image
                        .encoder()
                        .write_tag(tiff::tags::Tag::IccProfile, &icc[..])
                        .unwrap();
                }
                image.write_data(&$data).unwrap();
            }};
        }
        match page.colour {
            TiffColour::Rgb8 => write!(
                colortype::RGB8,
                px.iter()
                    .flat_map(|p| [p[0], p[1], p[2]])
                    .collect::<Vec<u8>>()
            ),
            TiffColour::Rgba8 => write!(colortype::RGBA8, rgba),
            TiffColour::Gray8 => write!(
                colortype::Gray8,
                px.iter().map(|p| p[0]).collect::<Vec<u8>>()
            ),
            TiffColour::Gray16 => write!(
                colortype::Gray16,
                px.iter()
                    .map(|p| u16::from(p[0]) * 257)
                    .collect::<Vec<u16>>()
            ),
            TiffColour::Cmyk8 => write!(
                colortype::CMYK8,
                px.iter()
                    .flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2], 0])
                    .collect::<Vec<u8>>()
            ),
        }
    }
    out.into_inner()
}

/// A HEIC of `screenshot_rgba`, tagged with an EXIF `orientation`; `None` when
/// this Mac has no HEIC encoder.
#[cfg(target_os = "macos")]
pub fn heic(w: u32, h: u32, orientation: u16) -> Option<Vec<u8>> {
    use crate::edit::raster::Raster;
    use crate::oscodec;
    use crate::sniff::Format;
    if !oscodec::can_encode(Format::Heic) {
        return None;
    }
    let r = Raster {
        width: w,
        height: h,
        rgba: screenshot_rgba(w, h),
        icc: None,
    };
    oscodec::macos::encode_oriented(&r, Format::Heic, 90, Some(orientation)).ok()
}

/// A one-page TIFF with `dpi` in its resolution tags, the unit being inches.
pub fn tiff_with_resolution(dpi: u32) -> Vec<u8> {
    use tiff::encoder::{Rational, TiffEncoder, colortype};
    use tiff::tags::{ResolutionUnit, Tag};
    let mut out = std::io::Cursor::new(Vec::new());
    let mut enc = TiffEncoder::new(&mut out).unwrap();
    let mut image = enc.new_image::<colortype::Gray8>(32, 32).unwrap();
    image.resolution(ResolutionUnit::Inch, Rational { n: dpi, d: 1 });
    image.encoder().write_tag(Tag::Orientation, 1_u16).unwrap();
    image.write_data(&[7_u8; 32 * 32]).unwrap();
    out.into_inner()
}

/// The X resolution of a TIFF's first page, when it is a whole number.
pub fn tiff_resolution(bytes: &[u8]) -> Option<u32> {
    use tiff::decoder::{Decoder, ifd::Value};
    let mut d = Decoder::new(std::io::Cursor::new(bytes)).ok()?;
    match d.find_tag(tiff::tags::Tag::XResolution).ok()?? {
        Value::Rational(n, 1) => Some(n),
        _ => None,
    }
}

// ---- stream B: audio fixtures ----
use crate::audio::tool::{AudioTool, ToolError};
use std::{
    collections::VecDeque,
    ffi::OsString,
    sync::{Arc, Mutex},
};

/// A canonical 16-bit PCM WAV: runs of a 440 Hz sine, each `(seconds, amplitude 0..1)`.
pub fn wav_tone(segments: &[(f32, f32)], rate: u32, channels: u16) -> Vec<u8> {
    let mut samples = Vec::new();
    let mut n = 0_u64;
    for &(secs, amp) in segments {
        for _ in 0..(secs * rate as f32).round() as u64 {
            let t = n as f32 / rate as f32;
            let v = (amp * (std::f32::consts::TAU * 440.0 * t).sin() * f32::from(i16::MAX)) as i16;
            for _ in 0..channels {
                samples.extend_from_slice(&v.to_le_bytes());
            }
            n += 1;
        }
    }
    let block = u32::from(channels) * 2;
    let mut out = Vec::with_capacity(44 + samples.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * block).to_le_bytes());
    out.extend_from_slice(&(block as u16).to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    out.extend_from_slice(&samples);
    out
}

/// What a `FakeTool` call does.
pub enum FakeStep {
    Log(String),
    /// Write `bytes` to the call's last argument.
    Write {
        log: String,
        bytes: Vec<u8>,
    },
    Fail(String),
}

/// An `AudioTool` that records its arguments and plays a script.
#[derive(Debug, Default)]
pub struct FakeTool {
    pub calls: Mutex<Vec<Vec<OsString>>>,
    pub script: Mutex<VecDeque<FakeStep>>,
    /// Cancel the token during the call with this index.
    pub cancel_on_call: Mutex<Option<usize>>,
}

impl std::fmt::Debug for FakeStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FakeStep")
    }
}

impl FakeTool {
    pub fn new(steps: Vec<FakeStep>) -> Arc<FakeTool> {
        Arc::new(FakeTool {
            script: Mutex::new(steps.into()),
            ..FakeTool::default()
        })
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|args| {
                args.iter()
                    .map(|a| a.to_string_lossy().into_owned())
                    .collect()
            })
            .collect()
    }
}

impl AudioTool for FakeTool {
    fn run(
        &self,
        args: &[OsString],
        cancel: &crate::cancel::CancelToken,
        on_progress: &mut dyn FnMut(f32),
    ) -> Result<String, ToolError> {
        let index = {
            let mut calls = self.calls.lock().unwrap();
            calls.push(args.to_vec());
            calls.len() - 1
        };
        if *self.cancel_on_call.lock().unwrap() == Some(index) {
            cancel.cancel();
            return Err(ToolError::Cancelled);
        }
        on_progress(0.5);
        on_progress(1.0);
        match self.script.lock().unwrap().pop_front() {
            None => Ok(String::new()),
            Some(FakeStep::Log(log)) => Ok(log),
            Some(FakeStep::Fail(why)) => Err(ToolError::Failed(why)),
            Some(FakeStep::Write { log, bytes }) => {
                if let Some(path) = args.last().filter(|a| *a != "-") {
                    std::fs::write(path, bytes).map_err(|e| ToolError::Failed(e.to_string()))?;
                }
                Ok(log)
            }
        }
    }
}

#[cfg(test)]
mod audio_fixture_tests {
    use super::*;
    use crate::cancel::CancelToken;

    #[test]
    fn wav_tone_is_a_valid_pcm_wav() {
        let w = wav_tone(&[(1.0, 0.5), (0.5, 0.1)], 8000, 2);
        assert_eq!(
            (&w[0..4], &w[8..12], &w[36..40]),
            (&b"RIFF"[..], &b"WAVE"[..], &b"data"[..])
        );
        assert_eq!(w.len(), 44 + 12_000 * 2 * 2);
    }

    #[test]
    fn the_fake_records_reports_progress_and_writes_the_last_argument() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.mp3");
        let fake = FakeTool::new(vec![FakeStep::Write {
            log: "done".into(),
            bytes: b"xyz".to_vec(),
        }]);
        let mut seen = Vec::new();
        let log = fake
            .run(
                &[OsString::from("-i"), out.clone().into()],
                &CancelToken::new(),
                &mut |p| seen.push(p),
            )
            .unwrap();
        assert_eq!(
            (log.as_str(), seen, std::fs::read(&out).unwrap()),
            ("done", vec![0.5, 1.0], b"xyz".to_vec())
        );
        assert_eq!(
            fake.calls(),
            vec![vec!["-i".to_string(), out.display().to_string()]]
        );
    }
}
