//! A GIF as full-canvas frames: each frame is composited onto the logical
//! screen the way a browser draws it, honouring disposal, so the animation can
//! be re-encoded as an animated WebP.

use gif::{DisposalMethod, Repeat};

use super::OptimiseError;
use super::webp_anim::{Animation, Frame, MAX_ANIMATION_BYTES};
use crate::i18n::t;

/// A GIF delay under two hundredths of a second plays as 100 ms in browsers.
const SLOW_DEFAULT_MS: u32 = 100;

pub fn decode(bytes: &[u8]) -> Result<Animation, OptimiseError> {
    let decode_err = |e: gif::DecodingError| OptimiseError::Decode(e.to_string());
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(bytes).map_err(decode_err)?;
    let (width, height) = (usize::from(decoder.width()), usize::from(decoder.height()));
    let canvas_bytes = (width * height * 4) as u64;
    if crate::sniff::gif_frame_count(bytes) as u64 * canvas_bytes > MAX_ANIMATION_BYTES {
        return Err(OptimiseError::Unsupported(t("pdf.anim.too_large")));
    }
    let mut canvas = vec![0_u8; width * height * 4];
    let mut frames = Vec::new();
    let mut loop_count = 0;
    while let Some(frame) = decoder.read_next_frame().map_err(decode_err)? {
        let before = (frame.dispose == DisposalMethod::Previous).then(|| canvas.clone());
        let (left, top) = (usize::from(frame.left), usize::from(frame.top));
        let (fw, fh) = (usize::from(frame.width), usize::from(frame.height));
        let cols = fw.min(width.saturating_sub(left));
        let rows = fh.min(height.saturating_sub(top));
        for row in 0..rows {
            for col in 0..cols {
                let src = (row * fw + col) * 4;
                let dst = ((top + row) * width + left + col) * 4;
                if frame.buffer[src + 3] > 0 {
                    canvas[dst..dst + 4].copy_from_slice(&frame.buffer[src..src + 4]);
                }
            }
        }
        let delay = u32::from(frame.delay);
        frames.push(Frame {
            rgba: canvas.clone(),
            duration_ms: if delay < 2 {
                SLOW_DEFAULT_MS
            } else {
                delay * 10
            },
        });
        match frame.dispose {
            DisposalMethod::Background => {
                for row in 0..rows {
                    let start = ((top + row) * width + left) * 4;
                    canvas[start..start + cols * 4].fill(0);
                }
            }
            DisposalMethod::Previous => {
                if let Some(before) = before {
                    canvas = before;
                }
            }
            DisposalMethod::Keep | DisposalMethod::Any => {}
        }
        loop_count = match decoder.repeat() {
            Repeat::Infinite => 0,
            Repeat::Finite(n) => u32::from(n),
        };
    }
    if frames.is_empty() {
        return Err(OptimiseError::Decode("the GIF has no frames".into()));
    }
    Ok(Animation {
        width: width as u32,
        height: height as u32,
        frames,
        loop_count,
        icc: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn frames_are_composited_on_the_screen() {
        let a = decode(&testutil::gif(3)).unwrap();
        assert_eq!((a.width, a.height, a.frames.len()), (64, 48, 3));
        assert!(
            a.frames
                .iter()
                .all(|f| f.rgba.len() == 64 * 48 * 4 && f.duration_ms >= 20)
        );
        assert_eq!(a.loop_count, 0);
    }

    #[test]
    fn background_disposal_clears_and_previous_restores() {
        // testutil::gif_with_disposal (frames counted from 0): frame 0 fills the screen red (Keep);
        // frame 1 draws a 4x4 blue square at 0,0 (Background); frame 2 draws a 1x1 green dot at
        // 10,10 (Previous); frame 3 draws a 1x1 white dot at 20,20 (Keep).
        let a = decode(&testutil::gif_with_disposal()).unwrap();
        let px = |f: usize, x: u32, y: u32| {
            a.frames[f].rgba[((y * a.width + x) * 4) as usize..][..4].to_vec()
        };
        assert_eq!(px(1, 0, 0), [0, 0, 255, 255]);
        assert_eq!(
            px(2, 0, 0),
            [0, 0, 0, 0],
            "frame 1's square was cleared to background"
        );
        assert_eq!(px(2, 10, 10), [0, 255, 0, 255]);
        assert_eq!(
            px(3, 10, 10),
            [255, 0, 0, 255],
            "frame 2's dot was undone back to red"
        );
        assert_eq!(px(3, 20, 20), [255, 255, 255, 255]);
    }

    #[test]
    fn a_fast_delay_plays_at_a_tenth_of_a_second() {
        let a = decode(&testutil::gif_with_disposal()).unwrap();
        assert!(a.frames.iter().all(|f| f.duration_ms == 100));
    }
}
