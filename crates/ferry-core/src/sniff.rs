//! Detect an image format from its content, never from the file extension (R§2.4).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Gif,
    WebP,
}

impl Format {
    /// File extension used when Ferry writes this format.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Gif => "gif",
            Format::WebP => "webp",
        }
    }
}

/// The format of `bytes`, or `None` when it is not a supported image.
pub fn sniff(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(Format::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Format::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(Format::Gif)
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(Format::WebP)
    } else {
        None
    }
}

/// True when the image has more than one frame: a GIF with several image
/// descriptors, a WebP with an animation chunk, or a PNG with an `acTL` chunk.
pub fn is_animated(bytes: &[u8], format: Format) -> bool {
    match format {
        Format::Png => png_has_actl(bytes),
        Format::Gif => gif_frame_count(bytes) > 1,
        Format::WebP => webp_has_animation(bytes),
        Format::Jpeg => false,
    }
}

fn png_has_actl(bytes: &[u8]) -> bool {
    let mut pos = 8; // after the signature
    while let Some(head) = bytes.get(pos..pos + 8) {
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        match &head[4..8] {
            b"acTL" => return true,
            b"IDAT" | b"IEND" => return false,
            _ => pos += 12 + len, // length + type + data + crc
        }
    }
    false
}

fn webp_has_animation(bytes: &[u8]) -> bool {
    let mut pos = 12; // after "RIFF<size>WEBP"
    while let Some(head) = bytes.get(pos..pos + 8) {
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        match &head[0..4] {
            b"ANIM" => return true,
            // VP8X flags: bit 1 is the animation flag.
            b"VP8X" if bytes.get(pos + 8).is_some_and(|flags| flags & 0x02 != 0) => return true,
            _ => pos += 8 + len + (len & 1), // chunks are padded to even length
        }
    }
    false
}

/// The number of image descriptors (frames) in a GIF; 0 for anything truncated
/// before its first frame.
pub fn gif_frame_count(bytes: &[u8]) -> usize {
    let Some(&screen_flags) = bytes.get(10) else {
        return 0;
    };
    let mut pos = 13;
    if screen_flags & 0x80 != 0 {
        pos += 3 << ((screen_flags & 7) + 1);
    }
    let mut frames = 0;
    while let Some(&block) = bytes.get(pos) {
        match block {
            0x21 => pos = skip_sub_blocks(bytes, pos + 2), // extension: label, then sub-blocks
            0x2C => {
                frames += 1;
                let Some(&image_flags) = bytes.get(pos + 9) else {
                    return frames;
                };
                pos += 10;
                if image_flags & 0x80 != 0 {
                    pos += 3 << ((image_flags & 7) + 1);
                }
                pos = skip_sub_blocks(bytes, pos + 1); // LZW minimum code size, then data
            }
            _ => return frames, // trailer (0x3B) or garbage
        }
    }
    frames
}

/// Position after a run of GIF data sub-blocks that starts at `pos`.
fn skip_sub_blocks(bytes: &[u8], mut pos: usize) -> usize {
    while let Some(&len) = bytes.get(pos) {
        pos += 1 + len as usize;
        if len == 0 {
            break;
        }
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_png() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"), Some(Format::Png));
    }

    #[test]
    fn detects_jpeg_even_when_it_would_be_named_png() {
        assert_eq!(
            sniff(&[0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x43]),
            Some(Format::Jpeg)
        );
    }

    #[test]
    fn detects_both_gif_versions() {
        assert_eq!(sniff(b"GIF87a\x01\0\x01\0"), Some(Format::Gif));
        assert_eq!(sniff(b"GIF89a\x01\0\x01\0"), Some(Format::Gif));
    }

    #[test]
    fn detects_webp() {
        assert_eq!(sniff(b"RIFF\x24\0\0\0WEBPVP8 "), Some(Format::WebP));
    }

    #[test]
    fn rejects_other_riff_files() {
        assert_eq!(sniff(b"RIFF\x24\0\0\0WAVEfmt "), None);
    }

    #[test]
    fn rejects_short_empty_and_text_input() {
        assert_eq!(sniff(b""), None);
        assert_eq!(sniff(b"\x89PN"), None);
        assert_eq!(sniff(b"RIFF\0\0\0\0WEB"), None);
        assert_eq!(sniff(b"hello world"), None);
    }

    #[test]
    fn extensions() {
        assert_eq!(Format::Png.extension(), "png");
        assert_eq!(Format::Jpeg.extension(), "jpg");
        assert_eq!(Format::Gif.extension(), "gif");
        assert_eq!(Format::WebP.extension(), "webp");
    }

    #[test]
    fn png_with_an_animation_control_chunk_is_animated() {
        let plain = crate::testutil::png(8, 8);
        assert!(!is_animated(&plain, Format::Png));
        let apng = crate::testutil::with_png_chunk(&plain, b"acTL", &[0, 0, 0, 2, 0, 0, 0, 0]);
        assert!(is_animated(&apng, Format::Png));
    }

    #[test]
    fn gif_is_animated_only_with_more_than_one_frame() {
        assert!(!is_animated(&crate::testutil::gif(1), Format::Gif));
        assert!(is_animated(&crate::testutil::gif(2), Format::Gif));
        assert!(!is_animated(b"GIF89a", Format::Gif)); // truncated
    }

    #[test]
    fn webp_is_animated_when_it_carries_an_anim_chunk() {
        assert!(is_animated(&crate::testutil::animated_webp(), Format::WebP));
        assert!(!is_animated(b"RIFF\x24\0\0\0WEBPVP8 ", Format::WebP));
    }

    #[test]
    fn jpeg_is_never_animated() {
        assert!(!is_animated(&[0xFF, 0xD8, 0xFF, 0xDB], Format::Jpeg));
    }
}
