//! Fit under size (R§2.5): the highest quality whose output is at most the
//! target, found with at most seven encodes.

use crate::optimise::OptimiseError;

const FLOOR: u8 = 5;

#[derive(Debug)]
pub struct Fitted {
    pub bytes: Vec<u8>,
    pub quality: u8,
    /// Even the floor quality is over the target; `bytes` is that smallest result.
    pub missed: bool,
}

/// Try the floor first (one encode settles an impossible target), then
/// binary-search 6..=100 with the six encodes left.
pub fn search(
    target: u64,
    mut encode: impl FnMut(u8) -> Result<Vec<u8>, OptimiseError>,
) -> Result<Fitted, OptimiseError> {
    let floor = encode(FLOOR)?;
    if floor.len() as u64 > target {
        return Ok(Fitted {
            bytes: floor,
            quality: FLOOR,
            missed: true,
        });
    }
    let mut best = (FLOOR, floor);
    let (mut lo, mut hi) = (FLOOR + 1, 100_u8);
    for _ in 0..6 {
        if lo > hi {
            break;
        }
        let q = lo + (hi - lo).div_ceil(2);
        let out = encode(q)?;
        if out.len() as u64 <= target {
            best = (q, out);
            lo = q + 1;
        } else {
            hi = q - 1;
        }
    }
    Ok(Fitted {
        bytes: best.1,
        quality: best.0,
        missed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake encoder whose output grows by 1000 bytes per quality step.
    fn sized(calls: &mut Vec<u8>) -> impl FnMut(u8) -> Result<Vec<u8>, OptimiseError> + '_ {
        move |q| {
            calls.push(q);
            Ok(vec![0; usize::from(q) * 1000])
        }
    }

    #[test]
    fn it_finds_a_high_quality_under_the_target_in_seven_encodes() {
        let mut calls = Vec::new();
        let fitted = search(50_500, sized(&mut calls)).unwrap();
        assert!(!fitted.missed);
        assert!(fitted.bytes.len() <= 50_500);
        assert!(fitted.quality >= 47, "quality {}", fitted.quality);
        assert!(calls.len() <= 7, "{calls:?}");
        assert_eq!(calls[0], 5, "the floor is tried first");
    }

    #[test]
    fn exactly_the_target_fits() {
        let mut calls = Vec::new();
        let fitted = search(100_000, sized(&mut calls)).unwrap();
        assert_eq!(
            (fitted.quality, fitted.bytes.len(), fitted.missed),
            (100, 100_000, false)
        );
    }

    #[test]
    fn an_unreachable_target_returns_the_smallest_and_says_so() {
        let mut calls = Vec::new();
        let fitted = search(1_000, sized(&mut calls)).unwrap();
        assert_eq!(
            (fitted.quality, fitted.bytes.len(), fitted.missed),
            (5, 5_000, true)
        );
        assert_eq!(calls, [5]);
    }

    #[test]
    fn an_encoder_error_stops_the_search() {
        let result = search(1_000, |_| Err(OptimiseError::Cancelled));
        assert!(matches!(result, Err(OptimiseError::Cancelled)));
    }
}
