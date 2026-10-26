//! The perceptual metric behind adaptive compression: SSIMULACRA2 (BSD-2-Clause)
//! over sample regions, so a 40 MP photo is never scored whole.

use ssimulacra2::{ColorPrimaries, Rgb, TransferCharacteristic, compute_frame_ssimulacra2};

use crate::edit::{geometry::Rect, raster::Raster};

/// Images up to this many pixels are scored whole.
const WHOLE_IMAGE: u64 = 1_048_576;
/// The side of a sample tile.
const TILE: u32 = 512;
/// The metric needs at least this many pixels per side.
const MIN_SIDE: u32 = 8;

#[derive(Debug)]
pub enum MetricError {
    TooSmall,
    SizeMismatch,
    Failed(String),
}

/// Where to look: the whole image when small, else four tiles centred on the
/// quadrant centres and clamped inside the image.
pub fn regions(width: u32, height: u32) -> Vec<Rect> {
    if u64::from(width) * u64::from(height) <= WHOLE_IMAGE {
        return vec![Rect {
            x: 0,
            y: 0,
            width,
            height,
        }];
    }
    let (tw, th) = (TILE.min(width), TILE.min(height));
    let centre = |c: u32, tile: u32, size: u32| (c.saturating_sub(tile / 2)).min(size - tile);
    [(1, 1), (3, 1), (1, 3), (3, 3)]
        .map(|(qx, qy)| Rect {
            x: centre(width * qx / 4, tw, width),
            y: centre(height * qy / 4, th, height),
            width: tw,
            height: th,
        })
        .to_vec()
}

/// The lowest SSIMULACRA2 score of `candidate` against `reference` over the
/// sample regions (100 is identical, about 80 is indistinguishable side by
/// side). Alpha is composited over mid-grey on both.
pub fn score(reference: &Raster, candidate: &Raster) -> Result<f64, MetricError> {
    if (reference.width, reference.height) != (candidate.width, candidate.height) {
        return Err(MetricError::SizeMismatch);
    }
    if reference.width < MIN_SIDE || reference.height < MIN_SIDE {
        return Err(MetricError::TooSmall);
    }
    let mut lowest = f64::MAX;
    for region in regions(reference.width, reference.height) {
        let a = tile(reference, region)?;
        let b = tile(candidate, region)?;
        let s = compute_frame_ssimulacra2(a, b).map_err(|e| MetricError::Failed(e.to_string()))?;
        lowest = lowest.min(s);
    }
    Ok(lowest)
}

/// One region as sRGB floats in 0..=1, composited over `#808080`.
fn tile(r: &Raster, region: Rect) -> Result<Rgb, MetricError> {
    let mut data = Vec::with_capacity(region.width as usize * region.height as usize);
    for y in region.y..region.y + region.height {
        let row = (y as usize * r.width as usize + region.x as usize) * 4;
        for p in r.rgba[row..row + region.width as usize * 4]
            .as_chunks::<4>()
            .0
        {
            let alpha = f32::from(p[3]) / 255.0;
            let over = |c: u8| f32::from(c) / 255.0 * alpha + 128.0 / 255.0 * (1.0 - alpha);
            data.push([over(p[0]), over(p[1]), over(p[2])]);
        }
    }
    Rgb::new(
        data,
        region.width as usize,
        region.height as usize,
        TransferCharacteristic::SRGB,
        ColorPrimaries::BT709,
    )
    .map_err(|e| MetricError::Failed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{optimise::OptimiseOptions, sniff::Format, testutil};

    fn raster(rgba: Vec<u8>, w: u32, h: u32) -> Raster {
        Raster {
            width: w,
            height: h,
            rgba,
            icc: None,
        }
    }

    fn jpeg_round_trip(r: &Raster, q: u8) -> Raster {
        let bytes = crate::edit::raster::encode(
            r,
            Format::Jpeg,
            &OptimiseOptions {
                compression: q,
                ..Default::default()
            },
        )
        .unwrap();
        crate::edit::raster::decode(&bytes, Format::Jpeg).unwrap()
    }

    #[test]
    fn an_image_scores_about_100_against_itself() {
        let r = raster(testutil::photo_rgba(128, 96), 128, 96);
        assert!(score(&r, &r).unwrap() > 99.0);
    }

    #[test]
    fn a_heavier_jpeg_scores_lower() {
        let r = raster(testutil::photo_rgba(256, 192), 256, 192);
        let (q30, q90) = (
            score(&r, &jpeg_round_trip(&r, 30)).unwrap(),
            score(&r, &jpeg_round_trip(&r, 90)).unwrap(),
        );
        assert!(q90 > q30 && q90 > 70.0, "q30 {q30}, q90 {q90}");
    }

    #[test]
    fn small_images_are_scored_whole_and_large_ones_in_four_tiles() {
        assert_eq!(
            regions(800, 600),
            [Rect {
                x: 0,
                y: 0,
                width: 800,
                height: 600
            }]
        );
        let tiles = regions(2000, 1500);
        assert_eq!(tiles.len(), 4);
        assert!(
            tiles.iter().all(|t| t.width == 512
                && t.height == 512
                && t.x + 512 <= 2000
                && t.y + 512 <= 1500)
        );
        assert_eq!(
            tiles[0],
            Rect {
                x: 244,
                y: 119,
                width: 512,
                height: 512
            }
        );
        assert!(
            regions(400, 3000)
                .iter()
                .all(|t| t.width == 400 && t.x == 0)
        );
    }

    #[test]
    fn colour_under_full_transparency_does_not_count() {
        let mut a = testutil::photo_rgba(64, 64);
        for px in a.chunks_mut(4).step_by(3) {
            px[3] = 0;
        }
        let mut b = a.clone();
        for px in b.chunks_mut(4).filter(|p| p[3] == 0) {
            px[0] = 255 - px[0];
        }
        assert!(score(&raster(a, 64, 64), &raster(b, 64, 64)).unwrap() > 99.0);
    }

    #[test]
    fn tiny_or_mismatched_images_are_errors() {
        let tiny = raster(vec![0; 4 * 4 * 4], 4, 4);
        assert!(matches!(score(&tiny, &tiny), Err(MetricError::TooSmall)));
        let a = raster(testutil::photo_rgba(16, 16), 16, 16);
        let b = raster(testutil::photo_rgba(16, 17), 16, 17);
        assert!(matches!(score(&a, &b), Err(MetricError::SizeMismatch)));
    }
}
