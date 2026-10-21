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
}
