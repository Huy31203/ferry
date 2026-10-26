//! PDF jobs (sE pdf-formats): the file structure is edited with lopdf, pages
//! are rendered with PDFium, which is loaded at run time from the bundled
//! library (`render::init`).

pub mod render;

use crate::settings;

/// The DPI stops of the card's slider, highest first.
pub const DPI_STOPS: [u16; 7] = [300, 250, 200, 150, 100, 72, 48];

/// The resolutions the settings give: images when optimising (normal and
/// aggressive) and pages extracted as images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Defaults {
    pub dpi: u16,
    pub dpi_aggressive: u16,
    pub extract_dpi: u16,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            dpi: 150,
            dpi_aggressive: 100,
            extract_dpi: 150,
        }
    }
}

impl From<&settings::Pdf> for Defaults {
    fn from(p: &settings::Pdf) -> Self {
        Self {
            dpi: p.dpi,
            dpi_aggressive: p.dpi_aggressive,
            extract_dpi: p.extract_dpi,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_settings_defaults() {
        assert_eq!(
            Defaults::from(&settings::Pdf::default()),
            Defaults::default()
        );
        assert!(DPI_STOPS.windows(2).all(|w| w[0] > w[1]));
    }
}
