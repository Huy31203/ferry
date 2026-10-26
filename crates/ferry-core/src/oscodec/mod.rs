//! The operating system's own image codecs, for formats Ferry cannot ship a
//! decoder for (HEIC): ImageIO on macOS. Nothing here links libheif; the OS
//! holds the HEVC licence.

#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "macos")]
pub use macos::{can_encode, decode, encode, reencode};

#[cfg(not(target_os = "macos"))]
mod other {
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

    /// Pixels as `format` at `quality`.
    pub fn encode(_r: &Raster, format: Format, _quality: u8) -> Result<Vec<u8>, OptimiseError> {
        Err(write_unsupported(format))
    }

    /// The file again at `quality`, keeping its orientation and colour.
    pub fn reencode(
        _bytes: &[u8],
        format: Format,
        _quality: u8,
        _strip_metadata: bool,
    ) -> Result<Vec<u8>, OptimiseError> {
        Err(write_unsupported(format))
    }

    fn write_unsupported(format: Format) -> OptimiseError {
        OptimiseError::Unsupported(tf("pdf.write_unsupported", &[("format", format.label())]))
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
}
#[cfg(not(target_os = "macos"))]
pub use other::{can_encode, decode, encode, reencode};
