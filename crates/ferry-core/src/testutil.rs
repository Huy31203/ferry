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
