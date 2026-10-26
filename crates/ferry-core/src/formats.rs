//! Which formats a card can convert to, and which conversions happen by
//! default (R§2.3): TIFF, HEIC and AVIF files, PDF page images, animations.

use serde::{Deserialize, Serialize};

use crate::settings;
use crate::sniff::Format;

/// A chip on the format bar and an item of "Convert to".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConvertChoice {
    pub format: Format,
    /// Only the first frame (or page) comes through.
    pub first_only: bool,
}

/// The targets a card of `source` offers, the source's own first when it is not
/// a standard target. Video and audio have their own lists, so they get none.
pub fn choices(
    source: Format,
    animated: bool,
    pages: u32,
    heic_encoder: bool,
) -> Vec<ConvertChoice> {
    if source.is_video() || source.is_audio() {
        return Vec::new();
    }
    if source == Format::Pdf {
        return [Format::Pdf, Format::Png, Format::Jpeg]
            .map(|format| ConvertChoice {
                format,
                first_only: false,
            })
            .to_vec();
    }
    // AVIF is read but not written yet (its encoder is a later cycle's), so it is
    // only ever offered as a source.
    let base = [Format::Png, Format::Jpeg, Format::WebP];
    let own = (!base.contains(&source)).then_some(source);
    let heic = (heic_encoder && source != Format::Heic).then_some(Format::Heic);
    own.into_iter()
        .chain(base)
        .chain(heic)
        .map(|format| ConvertChoice {
            format,
            first_only: format != source
                && ((animated && format != Format::WebP) || (source == Format::Tiff && pages > 1)),
        })
        .collect()
}

/// What a file of `source` becomes without being asked, from the settings.
pub fn default_target(source: Format, pages: u32, f: &settings::Formats) -> Option<Format> {
    match source {
        Format::Heic => f.heic_to.format(),
        Format::Tiff if pages <= 1 => f.tiff_to.format(),
        Format::Avif => f.avif_to.format(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formats(c: &[ConvertChoice]) -> Vec<Format> {
        c.iter().map(|c| c.format).collect()
    }

    #[test]
    fn a_png_offers_the_standard_targets() {
        assert_eq!(
            formats(&choices(Format::Png, false, 1, false)),
            [Format::Png, Format::Jpeg, Format::WebP]
        );
        assert_eq!(
            formats(&choices(Format::Png, false, 1, true)),
            [Format::Png, Format::Jpeg, Format::WebP, Format::Heic]
        );
        assert!(
            choices(Format::Jpeg, false, 1, true)
                .iter()
                .all(|c| !c.first_only)
        );
    }

    #[test]
    fn an_unusual_source_comes_first() {
        assert_eq!(
            formats(&choices(Format::Tiff, false, 1, false))[0],
            Format::Tiff
        );
        assert_eq!(
            formats(&choices(Format::Heic, false, 1, false)),
            [Format::Heic, Format::Png, Format::Jpeg, Format::WebP]
        );
        // A HEIC source stays first on a Mac that can also write HEIC.
        assert_eq!(
            formats(&choices(Format::Heic, false, 1, true)),
            [Format::Heic, Format::Png, Format::Jpeg, Format::WebP]
        );
        assert_eq!(
            formats(&choices(Format::Gif, true, 1, false)),
            [Format::Gif, Format::Png, Format::Jpeg, Format::WebP]
        );
    }

    #[test]
    fn a_pdf_converts_only_to_page_images() {
        assert_eq!(
            formats(&choices(Format::Pdf, false, 12, true)),
            [Format::Pdf, Format::Png, Format::Jpeg]
        );
    }

    #[test]
    fn video_and_audio_have_their_own_lists() {
        assert!(choices(Format::Mp4, false, 1, false).is_empty());
        assert!(choices(Format::Mp3, false, 1, false).is_empty());
    }

    #[test]
    fn stills_from_a_sequence_say_so() {
        let first =
            |c: &[ConvertChoice], f: Format| c.iter().find(|c| c.format == f).unwrap().first_only;
        let gif = choices(Format::Gif, true, 1, false);
        assert!(first(&gif, Format::Png) && first(&gif, Format::Jpeg));
        assert!(!first(&gif, Format::WebP) && !first(&gif, Format::Gif));
        let webp = choices(Format::WebP, true, 1, false);
        assert!(first(&webp, Format::Jpeg) && !first(&webp, Format::WebP));
        let tiff = choices(Format::Tiff, false, 3, false);
        assert!(first(&tiff, Format::Png) && !first(&tiff, Format::Tiff));
        assert!(!first(&choices(Format::Gif, false, 1, false), Format::Png));
    }

    #[test]
    fn default_targets_follow_the_settings() {
        let f = settings::Formats::default();
        assert_eq!(default_target(Format::Heic, 1, &f), Some(Format::Jpeg));
        assert_eq!(default_target(Format::Tiff, 1, &f), Some(Format::Jpeg));
        assert_eq!(default_target(Format::Tiff, 2, &f), None);
        assert_eq!(default_target(Format::Avif, 1, &f), None);
        let png = settings::Formats {
            avif_to: settings::ConvertTo::Png,
            ..f.clone()
        };
        assert_eq!(default_target(Format::Avif, 1, &png), Some(Format::Png));
        assert_eq!(default_target(Format::Png, 1, &f), None);
    }
}
