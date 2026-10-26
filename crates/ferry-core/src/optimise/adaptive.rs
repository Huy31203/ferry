//! Adaptive compression: the lowest quality whose result still scores at or
//! above a SSIMULACRA2 target against the original (stream F, plan D4).

use super::{Encoder, Format, Level, OptimiseError, OptimiseOptions, quality};
use crate::{
    edit::raster::{self, Raster},
    metric,
};

/// The qualities the search chooses from, lowest first.
pub const CANDIDATES: [u8; 14] = [30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 80, 85, 90, 95];
/// The most encodes one search may run.
pub const MAX_ENCODES: usize = 5;

/// 80 is "impossible to distinguish side by side at 1:1"; Aggressive accepts 70.
pub fn target(o: &OptimiseOptions) -> f64 {
    match o.level {
        Level::Aggressive => 70.0,
        Level::Normal => 80.0,
    }
}

/// Still PNG, JPEG and WebP only: gifsicle reshapes frames, and a visually
/// lossless encoder already fixes the quality.
pub fn applies(format: Format, o: &OptimiseOptions) -> bool {
    o.adaptive
        && matches!(format, Format::Png | Format::Jpeg | Format::WebP)
        && o.encoder != Some(Encoder::VisuallyLossless)
}

#[derive(Debug)]
pub struct Adapted {
    pub bytes: Vec<u8>,
    pub quality: u8,
    /// `None` when the metric failed and the settings quality was used.
    pub score: Option<f64>,
    /// Whether some candidate reached the target.
    pub reached: bool,
}

/// Search with SSIMULACRA2 against `reference`, the decoded original.
pub fn search(
    reference: &Raster,
    format: Format,
    o: &OptimiseOptions,
    encode: impl FnMut(&OptimiseOptions) -> Result<Vec<u8>, OptimiseError>,
) -> Result<Adapted, OptimiseError> {
    search_with(o, encode, |candidate| {
        let decoded = raster::decode(candidate, format).map_err(|e| e.to_string())?;
        metric::score(reference, &decoded).map_err(|e| format!("{e:?}"))
    })
}

/// The algorithm, with the encoder and the metric passed in.
pub fn search_with(
    o: &OptimiseOptions,
    mut encode: impl FnMut(&OptimiseOptions) -> Result<Vec<u8>, OptimiseError>,
    mut score: impl FnMut(&[u8]) -> Result<f64, String>,
) -> Result<Adapted, OptimiseError> {
    let cancelled = || o.cancel.is_cancelled().then_some(OptimiseError::Cancelled);
    let candidate = |q: u8| OptimiseOptions {
        compression: q,
        level: Level::Normal,
        adaptive: false,
        encoder: o
            .encoder
            .or((o.level == Level::Aggressive).then_some(Encoder::Small)),
        ..o.clone()
    };
    let goal = target(o);
    let (mut lo, mut hi) = (0_usize, CANDIDATES.len() - 1);
    let mut encodes = 0;
    let mut best: Option<Adapted> = None;
    let mut last_miss: Option<Adapted> = None;
    while lo <= hi && encodes < MAX_ENCODES {
        if let Some(e) = cancelled() {
            return Err(e);
        }
        let mid = usize::midpoint(lo, hi);
        let q = CANDIDATES[mid];
        let bytes = encode(&candidate(q))?;
        encodes += 1;
        if let Some(e) = cancelled() {
            return Err(e);
        }
        let Ok(s) = score(&bytes) else {
            return plain(o, encode);
        };
        let adapted = Adapted {
            bytes,
            quality: q,
            score: Some(s),
            reached: s >= goal,
        };
        if adapted.reached {
            best = Some(adapted);
            if mid == 0 {
                break;
            }
            hi = mid - 1;
        } else {
            lo = mid + 1;
            if mid == CANDIDATES.len() - 1 {
                last_miss = Some(adapted);
            }
        }
    }
    if let Some(best) = best {
        return Ok(best);
    }
    if let Some(top) = last_miss {
        return Ok(top);
    }
    if let Some(e) = cancelled() {
        return Err(e);
    }
    let top = CANDIDATES[CANDIDATES.len() - 1];
    Ok(Adapted {
        bytes: encode(&candidate(top))?,
        quality: top,
        score: None,
        reached: false,
    })
}

/// One plain encode at the settings quality, for when the metric cannot judge.
fn plain(
    o: &OptimiseOptions,
    mut encode: impl FnMut(&OptimiseOptions) -> Result<Vec<u8>, OptimiseError>,
) -> Result<Adapted, OptimiseError> {
    let fallback = OptimiseOptions {
        adaptive: false,
        ..o.clone()
    };
    Ok(Adapted {
        bytes: encode(&fallback)?,
        quality: quality(o),
        score: None,
        reached: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(
        calls: &mut Vec<u8>,
    ) -> impl FnMut(&OptimiseOptions) -> Result<Vec<u8>, OptimiseError> + '_ {
        move |o| {
            calls.push(o.compression);
            Ok(vec![o.compression; usize::from(o.compression)])
        }
    }

    fn on() -> OptimiseOptions {
        OptimiseOptions {
            adaptive: true,
            ..OptimiseOptions::default()
        }
    }

    #[test]
    fn it_picks_the_lowest_quality_that_reaches_the_target() {
        let mut calls = Vec::new();
        let a = search_with(&on(), fake(&mut calls), |b| Ok(f64::from(b[0]))).unwrap();
        assert_eq!((a.quality, a.reached), (80, true));
        assert!(calls.len() <= MAX_ENCODES, "{calls:?}");
        assert!(calls.iter().all(|q| CANDIDATES.contains(q)));
    }

    #[test]
    fn aggressive_aims_at_70() {
        let o = OptimiseOptions {
            level: Level::Aggressive,
            ..on()
        };
        let a = search_with(&o, fake(&mut Vec::new()), |b| Ok(f64::from(b[0]))).unwrap();
        assert_eq!(a.quality, 70);
    }

    #[test]
    fn when_nothing_reaches_it_the_top_quality_is_kept() {
        let a = search_with(&on(), fake(&mut Vec::new()), |_| Ok(10.0)).unwrap();
        assert_eq!((a.quality, a.reached, a.bytes.len()), (95, false, 95));
    }

    #[test]
    fn when_the_floor_already_reaches_it_the_floor_is_kept() {
        let a = search_with(&on(), fake(&mut Vec::new()), |_| Ok(100.0)).unwrap();
        assert_eq!(a.quality, 30);
    }

    #[test]
    fn a_metric_error_falls_back_to_the_settings_quality() {
        let o = OptimiseOptions {
            compression: 72,
            ..on()
        };
        let a = search_with(&o, fake(&mut Vec::new()), |_| Err("no".into())).unwrap();
        assert_eq!((a.quality, a.score, a.reached), (72, None, false));
    }

    #[test]
    fn a_cancel_stops_the_search() {
        let o = on();
        o.cancel.cancel();
        assert!(matches!(
            search_with(&o, fake(&mut Vec::new()), |_| Ok(90.0)),
            Err(OptimiseError::Cancelled)
        ));
    }
}
