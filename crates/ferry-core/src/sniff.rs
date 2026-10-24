//! Detect an image format from its content, never from the file extension (R§2.4).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Png,
    Jpeg,
    Gif,
    WebP,
    // stream A: video
    Mp4,
    Hevc,
    Av1,
    WebM,
    Mov,
    Mkv,
}

impl Format {
    /// File extension used when Ferry writes this format.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Gif => "gif",
            Format::WebP => "webp",
            // stream A: video
            Format::Mp4 | Format::Hevc => "mp4",
            Format::Av1 | Format::Mkv => "mkv",
            Format::WebM => "webm",
            Format::Mov => "mov",
        }
    }

    /// The name shown on cards: "PNG", "JPEG", "GIF", "WebP".
    pub fn label(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Gif => "GIF",
            Format::WebP => "WebP",
            // stream A: video
            Format::Mp4 => "MP4",
            Format::Hevc => "HEVC",
            Format::Av1 => "AV1",
            Format::WebM => "WebM",
            Format::Mov => "MOV",
            Format::Mkv => "MKV",
        }
    }

    /// MP4, HEVC, AV1, WebM, MOV and MKV.
    pub fn is_video(self) -> bool {
        matches!(
            self,
            Format::Mp4 | Format::Hevc | Format::Av1 | Format::WebM | Format::Mov | Format::Mkv
        )
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
        // stream A: video
        sniff_video(bytes)
    }
}

/// ISO BMFF brands that are video (`qt  ` is MOV). Others, such as HEIC, AVIF
/// and M4A, are left for the streams that own them.
const MP4_BRANDS: [&[u8; 4]; 25] = [
    b"isom", b"iso2", b"iso3", b"iso4", b"iso5", b"iso6", b"iso7", b"iso8", b"iso9", b"mp41",
    b"mp42", b"mp71", b"avc1", b"M4V ", b"M4VH", b"M4VP", b"MSNV", b"dash", b"3gp4", b"3gp5",
    b"3gp6", b"3g2a", b"mmp4", b"f4v ", b"XAVC",
];

// stream A: video
fn sniff_video(bytes: &[u8]) -> Option<Format> {
    if bytes.get(4..8) == Some(b"ftyp") {
        let brand: &[u8] = bytes.get(8..12)?;
        return if brand == b"qt  " {
            Some(Format::Mov)
        } else if MP4_BRANDS.iter().any(|b| b.as_slice() == brand) {
            Some(Format::Mp4)
        } else {
            None
        };
    }
    // Old QuickTime files start with an atom, not ftyp.
    if matches!(
        bytes.get(4..8),
        Some(b"moov" | b"mdat" | b"wide" | b"free" | b"skip")
    ) {
        return Some(Format::Mov);
    }
    if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        // The DocType element: 42 82, a one-byte size, then the name.
        let head = &bytes[..bytes.len().min(64)];
        let at = head.windows(2).position(|w| w == [0x42, 0x82])?;
        let size = usize::from(head.get(at + 2)? & 0x7F);
        return match head.get(at + 3..at + 3 + size)? {
            b"webm" => Some(Format::WebM),
            b"matroska" => Some(Format::Mkv),
            _ => None,
        };
    }
    None
}

/// True when the image has more than one frame: a GIF with several image
/// descriptors, a WebP with an animation chunk, or a PNG with an `acTL` chunk.
pub fn is_animated(bytes: &[u8], format: Format) -> bool {
    match format {
        Format::Png => png_has_actl(bytes),
        Format::Gif => gif_frame_count(bytes) > 1,
        Format::WebP => webp_has_animation(bytes),
        Format::Jpeg => false,
        // stream A: video
        Format::Mp4 | Format::Hevc | Format::Av1 | Format::WebM | Format::Mov | Format::Mkv => {
            false
        }
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
/// Whether every block is whole and the trailer is there. gifsicle only warns
/// about a truncated GIF and pads the missing frame data, so the optimiser
/// checks this first.
pub fn gif_is_complete(bytes: &[u8]) -> bool {
    let Some(&screen_flags) = bytes.get(10) else {
        return false;
    };
    let mut pos = 13;
    if screen_flags & 0x80 != 0 {
        pos += 3 << ((screen_flags & 7) + 1);
    }
    loop {
        let next = match bytes.get(pos) {
            Some(0x3B) => return true,
            Some(0x21) => whole_sub_blocks(bytes, pos + 2),
            Some(0x2C) => {
                let Some(&image_flags) = bytes.get(pos + 9) else {
                    return false;
                };
                pos += 10;
                if image_flags & 0x80 != 0 {
                    pos += 3 << ((image_flags & 7) + 1);
                }
                whole_sub_blocks(bytes, pos + 1) // LZW minimum code size, then data
            }
            _ => None,
        };
        let Some(next) = next else {
            return false;
        };
        pos = next;
    }
}

/// The position after a chain of sub-blocks, or `None` if it runs off the end
/// before its zero-length terminator.
fn whole_sub_blocks(bytes: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        let len = usize::from(*bytes.get(pos)?);
        pos += 1 + len;
        if len == 0 {
            return Some(pos);
        }
    }
}

fn skip_sub_blocks(bytes: &[u8], mut pos: usize) -> usize {
    while let Some(&len) = bytes.get(pos) {
        pos += 1 + len as usize;
        if len == 0 {
            break;
        }
    }
    pos
}

/// Width and height from the header alone, without decoding; `None` when the
/// header is cut short or not where the format puts it.
pub fn dimensions(bytes: &[u8], format: Format) -> Option<(u32, u32)> {
    match format {
        Format::Png => Some((be32(bytes, 16)?, be32(bytes, 20)?)),
        Format::Gif => Some((le16(bytes, 6)?, le16(bytes, 8)?)),
        Format::Jpeg => jpeg_dimensions(bytes),
        Format::WebP => webp_dimensions(bytes),
        // stream A: video
        Format::Mp4 | Format::Hevc | Format::Av1 | Format::WebM | Format::Mov | Format::Mkv => None,
    }
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn be16(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?).into())
}

fn le16(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?).into())
}

fn le24(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at + 3)?;
    Some(u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16)
}

/// The size in the first start-of-frame segment (SOF0 to SOF15, except the
/// DHT, JPG and DAC markers that share that range).
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut pos = 2;
    loop {
        if *bytes.get(pos)? != 0xFF {
            return None;
        }
        let marker = *bytes.get(pos + 1)?;
        match marker {
            0xFF => pos += 1,               // fill byte
            0x01 | 0xD0..=0xD8 => pos += 2, // no length
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                return Some((be16(bytes, pos + 7)?, be16(bytes, pos + 5)?));
            }
            _ => pos += 2 + be16(bytes, pos + 2)? as usize,
        }
    }
}

fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    match bytes.get(12..16)? {
        b"VP8X" => Some((le24(bytes, 24)? + 1, le24(bytes, 27)? + 1)),
        // Lossless: a 0x2F signature, then 14-bit width − 1 and height − 1.
        b"VP8L" if *bytes.get(20)? == 0x2F => {
            let bits = u32::from_le_bytes(bytes.get(21..25)?.try_into().ok()?);
            Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
        }
        // Lossy: a 3-byte frame tag and the 9D 01 2A start code, then 14-bit sizes.
        b"VP8 " if bytes.get(23..26)? == [0x9D, 0x01, 0x2A] => {
            Some((le16(bytes, 26)? & 0x3FFF, le16(bytes, 28)? & 0x3FFF))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_containers_are_sniffed_by_content() {
        let ftyp = |brand: &[u8; 4]| [&[0, 0, 0, 0x18][..], b"ftyp", brand, &[0; 12]].concat();
        assert_eq!(sniff(&ftyp(b"isom")), Some(Format::Mp4));
        assert_eq!(sniff(&ftyp(b"M4V ")), Some(Format::Mp4));
        assert_eq!(sniff(&ftyp(b"qt  ")), Some(Format::Mov));
        assert_eq!(
            sniff(&[&[0, 0, 0, 8][..], b"wide", &[0; 8]].concat()),
            Some(Format::Mov)
        );
        assert_eq!(sniff(&ftyp(b"heic")), None);
        assert_eq!(sniff(&ftyp(b"avif")), None);
        assert_eq!(sniff(&ftyp(b"M4A ")), None);
        let ebml = |doc: &[u8]| {
            [
                &[
                    0x1A,
                    0x45,
                    0xDF,
                    0xA3,
                    0x9F,
                    0x42,
                    0x86,
                    0x81,
                    0x01,
                    0x42,
                    0x82,
                    0x80 | doc.len() as u8,
                ][..],
                doc,
                &[0; 16],
            ]
            .concat()
        };
        assert_eq!(sniff(&ebml(b"webm")), Some(Format::WebM));
        assert_eq!(sniff(&ebml(b"matroska")), Some(Format::Mkv));
    }

    #[test]
    fn video_formats_have_names_extensions_and_serde_ids() {
        assert_eq!(
            (Format::Hevc.label(), Format::Hevc.extension()),
            ("HEVC", "mp4")
        );
        assert_eq!(
            (Format::Av1.label(), Format::Av1.extension()),
            ("AV1", "mkv")
        );
        assert_eq!(serde_json::to_string(&Format::WebM).unwrap(), "\"webm\"");
        assert_eq!(
            serde_json::from_str::<Format>("\"mov\"").unwrap(),
            Format::Mov
        );
        assert!(Format::Mkv.is_video() && !Format::Gif.is_video() && !Format::Png.is_video());
    }

    #[test]
    fn dimensions_come_from_the_header_of_every_format() {
        use crate::{optimise::webp::Extras, testutil};
        assert_eq!(
            dimensions(&testutil::png(37, 21), Format::Png),
            Some((37, 21))
        );
        assert_eq!(dimensions(&testutil::gif(1), Format::Gif), Some((64, 48)));
        let jpeg = testutil::jpeg(
            50,
            30,
            80,
            &testutil::JpegExtras {
                exif: Some(testutil::exif_with_orientation(6, 100)),
                ..Default::default()
            },
        );
        assert_eq!(dimensions(&jpeg, Format::Jpeg), Some((50, 30)));
        let lossy = testutil::webp(41, 19, 80, &Extras::default());
        assert_eq!(dimensions(&lossy, Format::WebP), Some((41, 19)));
        let alpha = testutil::webp_with_transparent_corner(40, 22);
        assert_eq!(dimensions(&alpha, Format::WebP), Some((40, 22)));
        let rgba = testutil::screenshot_rgba(33, 17);
        let lossless = webp::Encoder::from_rgba(&rgba, 33, 17)
            .encode_lossless()
            .to_vec();
        assert_eq!(dimensions(&lossless, Format::WebP), Some((33, 17)));
    }

    #[test]
    fn dimensions_of_a_cut_header_are_unknown() {
        let png = crate::testutil::png(8, 8);
        assert_eq!(dimensions(&png[..20], Format::Png), None);
        assert_eq!(dimensions(b"\xFF\xD8\xFF\xE0\x00", Format::Jpeg), None);
        assert_eq!(dimensions(b"RIFF\0\0\0\0WEBPVP8 ", Format::WebP), None);
    }

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

    #[test]
    fn a_gif_is_complete_only_when_every_block_and_the_trailer_are_there() {
        let full = crate::testutil::gif(2);
        assert!(gif_is_complete(&full));
        assert!(gif_is_complete(&crate::testutil::gif(1)));
        for cut in [0, 6, 13, 100, full.len() / 2, full.len() - 1] {
            assert!(!gif_is_complete(&full[..cut]), "cut at {cut}");
        }
        assert!(!gif_is_complete(b"not a gif"));
    }
}
