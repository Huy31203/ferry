//! The operating system's own image codecs, for formats Ferry cannot ship a
//! decoder for (HEIC): ImageIO on macOS, WIC on Windows. Nothing here links
//! libheif; the OS holds the HEVC licence.

use crate::edit::raster::Raster;
use crate::i18n::tf;
use crate::optimise::OptimiseError;
use crate::sniff::Format;

/// Whether this computer can write `format` through its OS codecs.
pub fn can_encode(_format: Format) -> bool {
    false
}

/// The picture of an OS-decoded file, oriented, with its colour profile.
pub fn decode(_bytes: &[u8], format: Format) -> Result<Raster, OptimiseError> {
    Err(OptimiseError::Unsupported(tf(
        "pdf.read_unsupported",
        &[("format", format.label())],
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_codec_nothing_is_written_or_read() {
        assert!(!can_encode(Format::Heic));
        assert!(matches!(
            decode(b"x", Format::Heic),
            Err(OptimiseError::Unsupported(_))
        ));
    }
}
