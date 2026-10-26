//! HEIC optimise: the OS codec writes the primary image again at the recipe's
//! quality, keeping orientation, colour and (unless stripping) metadata.

use super::{OptimiseError, OptimiseOptions, quality};
use crate::oscodec;
use crate::sniff::Format;

pub fn optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    oscodec::reencode(input, Format::Heic, quality(o), o.strip_metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn a_heic_comes_back_smaller_at_a_lower_quality() {
        let Some(input) = crate::testutil::heic(400, 300, 1) else {
            return;
        };
        let o = OptimiseOptions {
            compression: 20,
            ..OptimiseOptions::default()
        };
        let out = optimise(&input, &o).unwrap();
        assert!(out.len() < input.len(), "{} -> {}", input.len(), out.len());
        assert_eq!(crate::sniff::sniff(&out), Some(Format::Heic));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn without_a_codec_a_heic_is_not_written() {
        assert!(matches!(
            optimise(b"x", &OptimiseOptions::default()),
            Err(OptimiseError::Unsupported(_))
        ));
    }
}
