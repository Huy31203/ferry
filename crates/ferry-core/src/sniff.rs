//! Detect an image format from its content, never from the file extension (R§2.4).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Png,
    Jpeg,
    Gif,
    WebP,
    // sE pdf-formats
    Tiff,
    Heic,
    Avif,
    Pdf,
    // stream A: video
    Mp4,
    Hevc,
    Av1,
    WebM,
    Mov,
    Mkv,
    // ---- stream B: audio ----
    Mp3,
    M4a,
    Flac,
    Wav,
    Aiff,
    Ogg,
    Opus,
}

/// Bytes `Job::file` reads to sniff a file.
pub const HEAD: usize = 64 * 1024;

impl Format {
    // sE pdf-formats
    /// The still-image and document formats, as opposed to video and audio.
    pub const ALL: [Format; 8] = [
        Format::Png,
        Format::Jpeg,
        Format::Gif,
        Format::WebP,
        Format::Tiff,
        Format::Heic,
        Format::Avif,
        Format::Pdf,
    ];

    /// File extension used when Ferry writes this format.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Gif => "gif",
            Format::WebP => "webp",
            // sE pdf-formats
            Format::Tiff => "tiff",
            Format::Heic => "heic",
            Format::Avif => "avif",
            Format::Pdf => "pdf",
            // stream A: video
            Format::Mp4 | Format::Hevc => "mp4",
            Format::Av1 | Format::Mkv => "mkv",
            Format::WebM => "webm",
            Format::Mov => "mov",
            // ---- stream B: audio ----
            Format::Mp3 => "mp3",
            Format::M4a => "m4a",
            Format::Flac => "flac",
            Format::Wav => "wav",
            Format::Aiff => "aiff",
            Format::Ogg => "ogg",
            Format::Opus => "opus",
        }
    }

    /// The name shown on cards: "PNG", "JPEG", "GIF", "WebP".
    pub fn label(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Gif => "GIF",
            Format::WebP => "WebP",
            // sE pdf-formats
            Format::Tiff => "TIFF",
            Format::Heic => "HEIC",
            Format::Avif => "AVIF",
            Format::Pdf => "PDF",
            // stream A: video
            Format::Mp4 => "MP4",
            Format::Hevc => "HEVC",
            Format::Av1 => "AV1",
            Format::WebM => "WebM",
            Format::Mov => "MOV",
            Format::Mkv => "MKV",
            // ---- stream B: audio ----
            Format::Mp3 => "MP3",
            Format::M4a => "M4A",
            Format::Flac => "FLAC",
            Format::Wav => "WAV",
            Format::Aiff => "AIFF",
            Format::Ogg => "Ogg",
            Format::Opus => "Opus",
        }
    }

    /// MP4, HEVC, AV1, WebM, MOV and MKV.
    pub fn is_video(self) -> bool {
        matches!(
            self,
            Format::Mp4 | Format::Hevc | Format::Av1 | Format::WebM | Format::Mov | Format::Mkv
        )
    }

    // ---- stream B: audio ----
    /// MP3, M4A, FLAC, WAV, AIFF, Ogg and Opus.
    pub fn is_audio(self) -> bool {
        matches!(
            self,
            Format::Mp3
                | Format::M4a
                | Format::Flac
                | Format::Wav
                | Format::Aiff
                | Format::Ogg
                | Format::Opus
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
        // stream B: audio (before video: an M4A is an `ftyp` file too)
        sniff_audio(bytes)
            .or_else(|| sniff_video(bytes))
            // sE pdf-formats (HEIC and AVIF are `ftyp` files that are neither)
            .or_else(|| sniff_documents(bytes))
    }
}

// sE pdf-formats
/// TIFF (but not the camera RAW files built on it), HEIC, AVIF and PDF.
fn sniff_documents(bytes: &[u8]) -> Option<Format> {
    if matches!(
        bytes.get(..4),
        Some(b"II*\0" | b"MM\0*" | b"II+\0" | b"MM\0+")
    ) {
        // Canon CR2 says "CR" at byte 8; DNG and most other RAW files carry
        // DNGVersion (50706) or SubIFDs (330) in the first IFD.
        let raw = bytes.get(8..10) == Some(b"CR")
            || [50706, 330]
                .iter()
                .any(|&tag| tiff_tag(bytes, tag).is_some());
        return (!raw).then_some(Format::Tiff);
    }
    if let Some(format) = crate::isobmff::image_format(bytes) {
        return Some(format);
    }
    let head = &bytes[..bytes.len().min(1024)];
    head.windows(5)
        .any(|w| w == b"%PDF-")
        .then_some(Format::Pdf)
}

/// A classic TIFF's byte order (true = little endian) and its IFD offsets, as
/// far as they lie in `bytes`. BigTIFF has no readable IFDs here.
fn tiff_ifds(bytes: &[u8]) -> Option<(bool, Vec<usize>)> {
    let le = match bytes.get(..4)? {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let mut offsets = Vec::new();
    let mut next = tiff_u32(bytes, le, 4)? as usize;
    while next != 0 && offsets.len() < 10_000 && !offsets.contains(&next) {
        let Some(count) = tiff_u16(bytes, le, next) else {
            break;
        };
        offsets.push(next);
        next = tiff_u32(bytes, le, next + 2 + 12 * usize::from(count)).unwrap_or(0) as usize;
    }
    Some((le, offsets))
}

fn tiff_u16(bytes: &[u8], le: bool, at: usize) -> Option<u16> {
    let b = bytes.get(at..at + 2)?.try_into().ok()?;
    Some(if le {
        u16::from_le_bytes(b)
    } else {
        u16::from_be_bytes(b)
    })
}

fn tiff_u32(bytes: &[u8], le: bool, at: usize) -> Option<u32> {
    let b = bytes.get(at..at + 4)?.try_into().ok()?;
    Some(if le {
        u32::from_le_bytes(b)
    } else {
        u32::from_be_bytes(b)
    })
}

/// The value of `tag` in the first IFD when it is a single SHORT or LONG.
fn tiff_tag(bytes: &[u8], tag: u16) -> Option<u32> {
    let (le, offsets) = tiff_ifds(bytes)?;
    let ifd = *offsets.first()?;
    (0..usize::from(tiff_u16(bytes, le, ifd)?)).find_map(|i| {
        let entry = ifd + 2 + 12 * i;
        if tiff_u16(bytes, le, entry)? != tag {
            return None;
        }
        match tiff_u16(bytes, le, entry + 2)? {
            3 => tiff_u16(bytes, le, entry + 8).map(u32::from),
            4 => tiff_u32(bytes, le, entry + 8),
            _ => None,
        }
    })
}

/// The number of pages (IFDs) of a TIFF: 0 when `bytes` is not a TIFF, 1 for
/// BigTIFF or when the first IFD lies outside `bytes`.
pub fn tiff_page_count(bytes: &[u8]) -> u32 {
    match tiff_ifds(bytes) {
        Some((_, offsets)) => offsets.len().max(1) as u32,
        None if matches!(bytes.get(..4), Some(b"II+\0" | b"MM\0+")) => 1,
        None => 0,
    }
}

/// The EXIF orientation (1-8) of a TIFF's first page; 1 when it has none.
pub fn tiff_orientation(bytes: &[u8]) -> u16 {
    tiff_tag(bytes, 274)
        .and_then(|o| u16::try_from(o).ok())
        .filter(|o| (1..=8).contains(o))
        .unwrap_or(1)
}

// ---- stream B: audio ----
const AUDIO_BRANDS: [&[u8; 4]; 3] = [b"M4A ", b"M4B ", b"M4P "];
/// Major brands of video files that list `M4A ` among their compatible ones.
const VIDEO_MAJORS: [&[u8; 4]; 4] = [b"M4V ", b"M4VH", b"M4VP", b"qt  "];

fn sniff_audio(bytes: &[u8]) -> Option<Format> {
    let head = |range: std::ops::Range<usize>| bytes.get(range);
    if matches!(head(0..4), Some(b"RIFF" | b"RF64" | b"BW64")) && head(8..12) == Some(b"WAVE") {
        return Some(Format::Wav);
    }
    if head(0..4) == Some(b"FORM") && matches!(head(8..12), Some(b"AIFF" | b"AIFC")) {
        return Some(Format::Aiff);
    }
    if bytes.starts_with(b"fLaC") {
        return Some(Format::Flac);
    }
    if bytes.starts_with(b"ID3") {
        // A syncsafe size, then 10 more when a footer follows.
        let size = bytes.get(6..10).map_or(0, |b| {
            b.iter().fold(0, |n, &x| (n << 7) | usize::from(x & 0x7F))
        });
        let footer = if bytes.get(5).is_some_and(|f| f & 0x10 != 0) {
            10
        } else {
            0
        };
        return match bytes.get(10 + size + footer..) {
            Some(after) if after.starts_with(b"fLaC") => Some(Format::Flac),
            _ => Some(Format::Mp3),
        };
    }
    if bytes.starts_with(b"OggS") {
        let payload = bytes.get(27 + usize::from(*bytes.get(26)?)..)?;
        return if payload.starts_with(b"OpusHead") {
            Some(Format::Opus)
        } else if payload.starts_with(b"\x01vorbis") || payload.starts_with(b"\x7fFLAC") {
            Some(Format::Ogg)
        } else {
            None
        };
    }
    if head(4..8) == Some(b"ftyp") {
        let size = (be32(bytes, 0)? as usize).min(bytes.len());
        let major: &[u8] = bytes.get(8..12)?;
        let compatible = bytes
            .get(16..size)
            .unwrap_or_default()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|brand| AUDIO_BRANDS.contains(&brand));
        let audio_major = AUDIO_BRANDS.iter().any(|b| b.as_slice() == major);
        let video_major = VIDEO_MAJORS.iter().any(|b| b.as_slice() == major);
        return (audio_major || (compatible && !video_major)).then_some(Format::M4a);
    }
    // MPEG-1/2 Layer III frame sync (layer bits 00 are ADTS AAC).
    let [b0, b1, b2, ..] = *bytes else {
        return None;
    };
    (b0 == 0xFF
        && b1 & 0xE0 == 0xE0
        && (b1 >> 1) & 3 == 1
        && (b1 >> 3) & 3 != 1
        && b2 >> 4 != 0xF
        && (b2 >> 2) & 3 != 3)
        .then_some(Format::Mp3)
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
        // sE pdf-formats
        Format::Tiff | Format::Pdf => false,
        Format::Heic | Format::Avif => crate::isobmff::is_sequence(bytes),
        // stream A: video
        Format::Mp4 | Format::Hevc | Format::Av1 | Format::WebM | Format::Mov | Format::Mkv => {
            false
        }
        // ---- stream B: audio ----
        Format::Mp3
        | Format::M4a
        | Format::Flac
        | Format::Wav
        | Format::Aiff
        | Format::Ogg
        | Format::Opus => false,
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
        // sE pdf-formats
        Format::Tiff => Some((tiff_tag(bytes, 256)?, tiff_tag(bytes, 257)?)),
        Format::Heic | Format::Avif => crate::isobmff::primary_size(bytes),
        Format::Pdf => None,
        // stream A: video
        Format::Mp4 | Format::Hevc | Format::Av1 | Format::WebM | Format::Mov | Format::Mkv => None,
        // ---- stream B: audio ----
        Format::Mp3
        | Format::M4a
        | Format::Flac
        | Format::Wav
        | Format::Aiff
        | Format::Ogg
        | Format::Opus => None,
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
        // sE pdf-formats: these two are images now
        assert_eq!(sniff(&ftyp(b"heic")), Some(Format::Heic));
        assert_eq!(sniff(&ftyp(b"avif")), Some(Format::Avif));
        assert_eq!(sniff(&ftyp(b"M4A ")), Some(Format::M4a)); // stream B: audio
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
        assert_eq!(sniff(b"RIFF\x24\0\0\0RMIDdata"), None);
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

    // ---- stream B: audio ----
    fn ftyp(major: &[u8; 4], compat: &[&[u8; 4]]) -> Vec<u8> {
        let size = 16 + 4 * compat.len();
        let mut b = (size as u32).to_be_bytes().to_vec();
        b.extend_from_slice(b"ftyp");
        b.extend_from_slice(major);
        b.extend_from_slice(&[0, 0, 2, 0]);
        for c in compat {
            b.extend_from_slice(*c);
        }
        b
    }

    fn ogg(packet: &[u8]) -> Vec<u8> {
        let mut b = b"OggS\0\x02".to_vec();
        b.extend_from_slice(&[0; 20]);
        b.push(1);
        b.push(packet.len() as u8);
        b.extend_from_slice(packet);
        b
    }

    #[test]
    fn detects_each_audio_container() {
        assert_eq!(
            sniff(&crate::testutil::wav_tone(&[(0.01, 0.5)], 8000, 1)),
            Some(Format::Wav)
        );
        assert_eq!(sniff(b"RF64\xff\xff\xff\xffWAVEds64"), Some(Format::Wav));
        assert_eq!(sniff(b"FORM\0\0\0\x10AIFFCOMM"), Some(Format::Aiff));
        assert_eq!(sniff(b"FORM\0\0\0\x10AIFCFVER"), Some(Format::Aiff));
        assert_eq!(sniff(b"fLaC\0\0\0\x22"), Some(Format::Flac));
        assert_eq!(
            sniff(b"ID3\x03\0\0\0\0\0\x00\xff\xfb\x90\x64"),
            Some(Format::Mp3)
        );
        assert_eq!(sniff(b"ID3\x04\0\0\0\0\0\x00fLaC"), Some(Format::Flac));
        assert_eq!(sniff(&[0xFF, 0xFB, 0x90, 0x64, 0, 0]), Some(Format::Mp3));
        assert_eq!(sniff(&[0xFF, 0xF3, 0x48, 0xC4, 0, 0]), Some(Format::Mp3));
        assert_eq!(sniff(&ogg(b"OpusHead\x01\x02")), Some(Format::Opus));
        assert_eq!(sniff(&ogg(b"\x01vorbis\0\0")), Some(Format::Ogg));
        assert_eq!(
            sniff(&ftyp(b"M4A ", &[b"M4A ", b"mp42", b"isom"])),
            Some(Format::M4a)
        );
        assert_eq!(
            sniff(&ftyp(b"mp42", &[b"isom", b"M4A "])),
            Some(Format::M4a)
        );
        assert_eq!(sniff(&ftyp(b"M4B ", &[])), Some(Format::M4a));
    }

    #[test]
    fn video_and_raw_streams_are_not_audio() {
        let not_audio = |b: &[u8]| !sniff(b).is_some_and(Format::is_audio);
        assert!(not_audio(&ftyp(
            b"isom",
            &[b"isom", b"iso2", b"avc1", b"mp41"]
        )));
        assert!(not_audio(&ftyp(b"qt  ", &[b"qt  "])));
        // iTunes video lists M4A among its compatible brands.
        assert!(not_audio(&ftyp(b"M4V ", &[b"M4V ", b"M4A ", b"mp42"])));
        assert!(not_audio(&[0xFF, 0xF1, 0x50, 0x80, 0, 0]));
        assert!(not_audio(b"RIFF\x24\0\0\0RMIDdata"));
        assert!(not_audio(&ogg(b"\x80theora")));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(Format::Jpeg));
    }

    #[test]
    fn audio_formats_have_names_and_no_pixels() {
        for (f, ext, label) in [
            (Format::Mp3, "mp3", "MP3"),
            (Format::M4a, "m4a", "M4A"),
            (Format::Flac, "flac", "FLAC"),
            (Format::Wav, "wav", "WAV"),
            (Format::Aiff, "aiff", "AIFF"),
            (Format::Ogg, "ogg", "Ogg"),
            (Format::Opus, "opus", "Opus"),
        ] {
            assert_eq!((f.extension(), f.label(), f.is_audio()), (ext, label, true));
            assert_eq!(serde_json::to_string(&f).unwrap(), format!("\"{ext}\""));
            assert!(!is_animated(b"", f) && dimensions(b"", f).is_none());
        }
        assert!(!Format::Png.is_audio());
    }

    // sE pdf-formats
    #[test]
    fn detects_tiff_in_both_byte_orders() {
        use crate::testutil;
        assert_eq!(
            sniff(&testutil::tiff_ifds(&[(4, 3)], None)),
            Some(Format::Tiff)
        );
        let mut be = b"MM\0*\0\0\0\x08".to_vec();
        be.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        assert_eq!(sniff(&be), Some(Format::Tiff));
    }

    #[test]
    fn camera_raw_files_are_not_tiff() {
        use crate::testutil;
        assert_eq!(sniff(b"II*\0\x10\0\0\0CR\x02\0\0\0\0\0"), None);
        assert_eq!(sniff(&testutil::tiff_ifds(&[(4, 3)], Some(50706))), None);
        assert_eq!(sniff(&testutil::tiff_ifds(&[(4, 3)], Some(330))), None);
    }

    #[test]
    fn detects_heic_and_avif_by_brand() {
        use crate::testutil::ftyp;
        assert_eq!(
            sniff(&ftyp(b"heic", &[b"mif1", b"heic"])),
            Some(Format::Heic)
        );
        assert_eq!(
            sniff(&ftyp(b"avif", &[b"mif1", b"miaf"])),
            Some(Format::Avif)
        );
        assert_eq!(sniff(&ftyp(b"mif1", &[b"avif"])), Some(Format::Avif));
        // An MP4 is a video (stream A), never a HEIC.
        assert_eq!(sniff(&ftyp(b"isom", &[b"mp41"])), Some(Format::Mp4));
    }

    #[test]
    fn detects_pdf_even_after_leading_bytes() {
        assert_eq!(sniff(b"%PDF-1.7\n%\xE2\xE3"), Some(Format::Pdf));
        let mut junk = vec![b' '; 100];
        junk.extend_from_slice(b"%PDF-1.4\n");
        assert_eq!(sniff(&junk), Some(Format::Pdf));
        let mut late = vec![b' '; 1100];
        late.extend_from_slice(b"%PDF-1.4\n");
        assert_eq!(sniff(&late), None);
    }

    #[test]
    fn image_sequences_are_animated() {
        use crate::testutil::{ftyp, tiff_ifds};
        assert!(is_animated(&ftyp(b"avis", &[b"avif"]), Format::Avif));
        assert!(is_animated(&ftyp(b"msf1", &[b"heic"]), Format::Heic));
        assert!(!is_animated(&ftyp(b"heic", &[b"mif1"]), Format::Heic));
        assert!(!is_animated(
            &tiff_ifds(&[(4, 3), (4, 3)], None),
            Format::Tiff
        ));
    }

    #[test]
    fn every_image_and_document_format_has_a_name_an_extension_and_a_serde_name() {
        let names: Vec<_> = Format::ALL
            .iter()
            .map(|f| serde_json::to_value(f).unwrap())
            .collect();
        assert_eq!(
            names,
            ["png", "jpeg", "gif", "webp", "tiff", "heic", "avif", "pdf"]
        );
        assert_eq!(
            (
                Format::Tiff.extension(),
                Format::Heic.label(),
                Format::Pdf.extension()
            ),
            ("tiff", "HEIC", "pdf")
        );
    }

    #[test]
    fn tiff_size_pages_and_orientation_come_from_the_ifds() {
        use crate::testutil;
        let two = testutil::tiff_ifds(&[(40, 30), (20, 10)], None);
        assert_eq!(dimensions(&two, Format::Tiff), Some((40, 30)));
        assert_eq!(tiff_page_count(&two), 2);
        assert_eq!(tiff_orientation(&two), 1);
        assert_eq!(
            tiff_orientation(&testutil::tiff_ifds(&[(4, 3)], Some(274))),
            1
        );
        assert_eq!(tiff_page_count(b"not a tiff"), 0);
        assert_eq!(tiff_page_count(b"II+\0\x08\0\x08\0"), 1);
    }

    #[test]
    fn a_cut_short_or_looping_tiff_is_not_a_panic() {
        use crate::testutil;
        let full = testutil::tiff_ifds(&[(40, 30), (20, 10)], None);
        for len in 0..full.len() {
            let _ = (sniff(&full[..len]), tiff_page_count(&full[..len]));
        }
        // The first IFD points back at itself.
        let mut looped = testutil::tiff_ifds(&[(40, 30)], None);
        let end = looped.len();
        looped[end - 4..].copy_from_slice(&8_u32.to_le_bytes());
        assert_eq!(tiff_page_count(&looped), 1);
    }
}
