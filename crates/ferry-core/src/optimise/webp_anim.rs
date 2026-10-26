//! Animated WebP through libwebp's animation API: frames decode to full
//! canvases, and `WebPAnimEncoder` writes them again, keeping every frame's
//! duration (the `webp` crate's `AnimEncoder` loses the last one) and the
//! loop count. An optimisation or edit that would change the frame count
//! fails, so an animation is never flattened.

use std::os::raw::c_int;
use std::ptr;

use libwebp_sys as sys;

use super::png::MAX_PIXELS;
use super::webp::{self, Extras, chunks, push_chunk};
use super::{OptimiseError, OptimiseOptions};
use crate::edit::raster::Raster;
use crate::i18n::t;

/// Decoded frames of one animation may take at most this many bytes.
pub const MAX_ANIMATION_BYTES: u64 = 1_000_000_000;

/// A frame that says it ends at the time the last one did gets this long.
const DEFAULT_DURATION_MS: u32 = 100;

pub struct Animation {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<Frame>,
    /// 0 loops forever.
    pub loop_count: u32,
    pub icc: Option<Vec<u8>>,
}

/// A full canvas of straight RGBA, shown for `duration_ms`.
pub struct Frame {
    pub rgba: Vec<u8>,
    pub duration_ms: u32,
}

/// The number of frames (`ANMF` chunks) of an animated WebP.
pub fn frame_count(bytes: &[u8]) -> usize {
    chunks(bytes).iter().filter(|(n, _)| n == b"ANMF").count()
}

/// The canvas size from the `VP8X` chunk.
fn canvas(bytes: &[u8]) -> Option<(u32, u32)> {
    let (_, vp8x) = *chunks(bytes).iter().find(|(n, _)| n == b"VP8X")?;
    let le24 = |at: usize| {
        let b = vp8x.get(at..at + 3)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], 0]) + 1)
    };
    Some((le24(4)?, le24(7)?))
}

fn decode_err(why: &str) -> OptimiseError {
    OptimiseError::Decode(format!("not a readable animated WebP: {why}"))
}

/// Every frame of the animation. The size is checked from the headers
/// before any buffer is allocated.
pub fn decode(bytes: &[u8]) -> Result<Animation, OptimiseError> {
    let (w, h) = canvas(bytes).ok_or_else(|| decode_err("no canvas"))?;
    if frame_count(bytes) as u64 * u64::from(w) * u64::from(h) * 4 > MAX_ANIMATION_BYTES {
        return Err(OptimiseError::Unsupported(t("pdf.anim.too_large")));
    }
    decode_frames(bytes, usize::MAX)
}

/// Frame 0 with the file's ICC profile.
pub fn first_frame(bytes: &[u8]) -> Result<Raster, OptimiseError> {
    let (w, h) = canvas(bytes).ok_or_else(|| decode_err("no canvas"))?;
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(OptimiseError::Unsupported(format!(
            "{w} x {h} pixels is too large to edit"
        )));
    }
    let a = decode_frames(bytes, 1)?;
    let frame = a
        .frames
        .into_iter()
        .next()
        .ok_or_else(|| decode_err("no frames"))?;
    Ok(Raster {
        width: a.width,
        height: a.height,
        rgba: frame.rgba,
        icc: a.icc,
    })
}

/// Frees the animation decoder on every path.
struct DecoderGuard(*mut sys::WebPAnimDecoder);

impl Drop for DecoderGuard {
    fn drop(&mut self) {
        // SAFETY: the pointer came from WebPAnimDecoderNew and is freed once.
        unsafe { sys::WebPAnimDecoderDelete(self.0) }
    }
}

fn decode_frames(bytes: &[u8], limit: usize) -> Result<Animation, OptimiseError> {
    // SAFETY: zeroed options are initialised by the call below.
    let mut options: sys::WebPAnimDecoderOptions = unsafe { std::mem::zeroed() };
    // SAFETY: `options` is a valid, writable struct.
    if unsafe { sys::WebPAnimDecoderOptionsInit(&mut options) } == 0 {
        return Err(decode_err("libwebp options"));
    }
    options.color_mode = sys::WEBP_CSP_MODE::MODE_RGBA;
    options.use_threads = 0;
    let data = sys::WebPData {
        bytes: bytes.as_ptr(),
        size: bytes.len(),
    };
    // SAFETY: `data` points at `bytes`, which outlives the decoder.
    let decoder = unsafe { sys::WebPAnimDecoderNew(&data, &options) };
    if decoder.is_null() {
        return Err(decode_err("libwebp could not open it"));
    }
    let decoder = DecoderGuard(decoder);
    let mut info = sys::WebPAnimInfo::default();
    // SAFETY: a live decoder and a writable struct.
    if unsafe { sys::WebPAnimDecoderGetInfo(decoder.0, &mut info) } == 0 {
        return Err(decode_err("no info"));
    }
    let len = info.canvas_width as usize * info.canvas_height as usize * 4;
    let mut frames = Vec::new();
    let mut previous_end = 0;
    // SAFETY: a live decoder.
    while frames.len() < limit && unsafe { sys::WebPAnimDecoderHasMoreFrames(decoder.0) } != 0 {
        let mut buffer: *mut u8 = ptr::null_mut();
        let mut end: c_int = 0;
        // SAFETY: a live decoder; the buffer it hands back holds one canvas and stays
        // valid until the next call, so it is copied at once.
        if unsafe { sys::WebPAnimDecoderGetNext(decoder.0, &mut buffer, &mut end) } == 0 {
            return Err(decode_err("a frame is damaged"));
        }
        // SAFETY: `buffer` holds `len` bytes (canvas_width * canvas_height * 4).
        let rgba = unsafe { std::slice::from_raw_parts(buffer, len) }.to_vec();
        let duration = u32::try_from(end - previous_end).unwrap_or(0);
        previous_end = end;
        frames.push(Frame {
            rgba,
            duration_ms: if duration == 0 {
                DEFAULT_DURATION_MS
            } else {
                duration
            },
        });
    }
    Ok(Animation {
        width: info.canvas_width,
        height: info.canvas_height,
        frames,
        loop_count: info.loop_count,
        icc: chunks(bytes)
            .iter()
            .find(|(n, _)| n == b"ICCP")
            .map(|(_, d)| d.to_vec()),
    })
}

/// Lossy at the recipe's quality, method 4; the ICC profile and, unless
/// stripping, EXIF and XMP come from `extras`.
pub fn encode(
    a: &Animation,
    o: &OptimiseOptions,
    extras: &Extras,
) -> Result<Vec<u8>, OptimiseError> {
    encode_with(a, &webp::config(o)?, o, extras)
}

fn encode_with(
    a: &Animation,
    config: &sys::WebPConfig,
    o: &OptimiseOptions,
    extras: &Extras,
) -> Result<Vec<u8>, OptimiseError> {
    let encode_err = |why: &str| OptimiseError::Encode(format!("animated WebP: {why}"));
    let too_big = || OptimiseError::Unsupported(t("pdf.anim.too_large"));
    let (width, height) = (
        c_int::try_from(a.width).map_err(|_| too_big())?,
        c_int::try_from(a.height).map_err(|_| too_big())?,
    );
    // SAFETY: zeroed options are initialised by the call below.
    let mut options: sys::WebPAnimEncoderOptions = unsafe { std::mem::zeroed() };
    // SAFETY: `options` is a valid, writable struct.
    if unsafe { sys::WebPAnimEncoderOptionsInitInternal(&mut options, sys::WebPGetMuxABIVersion()) }
        == 0
    {
        return Err(encode_err("libwebp options"));
    }
    options.anim_params.loop_count = c_int::try_from(a.loop_count).unwrap_or(0);
    options.allow_mixed = 1;
    // SAFETY: valid size and options.
    let encoder = unsafe {
        sys::WebPAnimEncoderNewInternal(width, height, &options, sys::WebPGetMuxABIVersion())
    };
    if encoder.is_null() {
        return Err(encode_err("libwebp could not start"));
    }
    let encoder = EncoderGuard(encoder);
    let mut start_ms: c_int = 0;
    for frame in &a.frames {
        if o.cancel.is_cancelled() {
            return Err(OptimiseError::Cancelled);
        }
        let mut picture = PictureGuard::new()?;
        picture.0.use_argb = 1;
        picture.0.width = width;
        picture.0.height = height;
        // SAFETY: the picture is initialised; the frame holds `width * 4` bytes a row.
        if unsafe { sys::WebPPictureImportRGBA(&mut picture.0, frame.rgba.as_ptr(), width * 4) }
            == 0
        {
            return Err(encode_err("no memory for a frame"));
        }
        // SAFETY: a live encoder, picture and config.
        if unsafe { sys::WebPAnimEncoderAdd(encoder.0, &mut picture.0, start_ms, config) } == 0 {
            return Err(encode_err("a frame did not encode"));
        }
        start_ms = start_ms.saturating_add(c_int::try_from(frame.duration_ms).unwrap_or(0));
    }
    // A null frame at the end time gives the last frame its duration.
    // SAFETY: libwebp documents a null frame and config as the end marker.
    if unsafe { sys::WebPAnimEncoderAdd(encoder.0, ptr::null_mut(), start_ms, ptr::null()) } == 0 {
        return Err(encode_err("the end of the animation did not encode"));
    }
    let mut out = sys::WebPData::default();
    // SAFETY: a live encoder and a writable struct.
    if unsafe { sys::WebPAnimEncoderAssemble(encoder.0, &mut out) } == 0 {
        return Err(encode_err("the animation did not assemble"));
    }
    // SAFETY: libwebp filled `out` with `size` bytes at `bytes`.
    let assembled = unsafe { std::slice::from_raw_parts(out.bytes, out.size) }.to_vec();
    // SAFETY: `out` came from the assemble call and is freed once.
    unsafe { sys::WebPDataClear(&mut out) };
    Ok(with_extras(&assembled, extras))
}

struct EncoderGuard(*mut sys::WebPAnimEncoder);

impl Drop for EncoderGuard {
    fn drop(&mut self) {
        // SAFETY: the pointer came from WebPAnimEncoderNewInternal and is freed once.
        unsafe { sys::WebPAnimEncoderDelete(self.0) }
    }
}

struct PictureGuard(sys::WebPPicture);

impl PictureGuard {
    fn new() -> Result<Self, OptimiseError> {
        sys::WebPPicture::new()
            .map(Self)
            .map_err(|()| OptimiseError::Encode("libwebp picture".into()))
    }
}

impl Drop for PictureGuard {
    fn drop(&mut self) {
        // SAFETY: the picture was initialised; freeing it twice is harmless in libwebp.
        unsafe { sys::WebPPictureFree(&mut self.0) }
    }
}

/// `ICCP` directly after `VP8X`, `EXIF` and `XMP ` at the end, and the
/// `VP8X` flags to match.
fn with_extras(webp: &[u8], extras: &Extras) -> Vec<u8> {
    let parts = chunks(webp);
    let Some(((_, vp8x), rest)) = parts.split_first().filter(|((n, _), _)| n == b"VP8X") else {
        return webp.to_vec();
    };
    if extras.icc.is_none() && extras.exif.is_none() && extras.xmp.is_none() {
        return webp.to_vec();
    }
    let mut head = vp8x.to_vec();
    for (present, flag) in [
        (extras.icc.is_some(), webp::ICC_FLAG),
        (extras.exif.is_some(), webp::EXIF_FLAG),
        (extras.xmp.is_some(), webp::XMP_FLAG),
    ] {
        if present && let Some(flags) = head.first_mut() {
            *flags |= flag;
        }
    }
    let mut body = b"WEBP".to_vec();
    push_chunk(&mut body, b"VP8X", &head);
    if let Some(icc) = &extras.icc {
        push_chunk(&mut body, b"ICCP", icc);
    }
    for (name, data) in rest {
        push_chunk(&mut body, name, data);
    }
    if let Some(exif) = &extras.exif {
        push_chunk(&mut body, b"EXIF", exif);
    }
    if let Some(xmp) = &extras.xmp {
        push_chunk(&mut body, b"XMP ", xmp);
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Re-encode the animation. A result with another frame count is an error, so
/// the original is kept (R§2.4).
pub fn optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let animation = decode(input)?;
    let original = chunks(input);
    let find = |name: &[u8; 4]| {
        original
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, data)| data.to_vec())
            .filter(|_| !o.strip_metadata)
    };
    let extras = Extras {
        icc: animation.icc.clone(),
        exif: find(b"EXIF"),
        xmp: find(b"XMP "),
    };
    let out = encode(&animation, o, &extras)?;
    if frame_count(&out) != frame_count(input) {
        return Err(OptimiseError::Encode(t("pdf.anim.flattened")));
    }
    Ok(out)
}

/// A lossless encode, for fixtures.
#[cfg(test)]
pub(crate) fn encode_lossless(a: &Animation) -> Vec<u8> {
    let mut config = webp::config(&OptimiseOptions::default()).unwrap();
    config.lossless = 1;
    config.quality = 100.0;
    encode_with(a, &config, &OptimiseOptions::default(), &Extras::default()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn an_animation_round_trips_its_frames_timing_and_loop() {
        let bytes = testutil::animated_webp_frames(&[100, 200, 300], 64, 48);
        let a = decode(&bytes).unwrap();
        assert_eq!((a.width, a.height, a.frames.len()), (64, 48, 3));
        assert_eq!(
            a.frames.iter().map(|f| f.duration_ms).collect::<Vec<_>>(),
            [100, 200, 300]
        );
        assert_eq!(a.loop_count, 0);
    }

    #[test]
    fn optimising_keeps_every_frame_and_the_last_frames_duration() {
        let input = testutil::animated_webp_frames(&[100, 200, 300], 160, 120);
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert_eq!(frame_count(&out), 3);
        assert_eq!(
            decode(&out).unwrap().frames.last().unwrap().duration_ms,
            300
        );
        assert!(crate::sniff::is_animated(&out, crate::sniff::Format::WebP));
    }

    #[test]
    fn the_colour_profile_survives() {
        let mut a = decode(&testutil::animated_webp_frames(&[100, 100], 32, 32)).unwrap();
        a.icc = Some(vec![5; 90]);
        let extras = Extras {
            icc: a.icc.clone(),
            ..Extras::default()
        };
        let out = encode(&a, &OptimiseOptions::default(), &extras).unwrap();
        assert_eq!(decode(&out).unwrap().icc, Some(vec![5; 90]));
    }

    #[test]
    fn exif_follows_the_metadata_setting() {
        let a = decode(&testutil::animated_webp_frames(&[100, 100], 32, 32)).unwrap();
        let extras = Extras {
            exif: Some(vec![1, 2, 3, 4]),
            ..Extras::default()
        };
        let with = encode(&a, &OptimiseOptions::default(), &extras).unwrap();
        let has = |b: &[u8], n: &[u8; 4]| chunks(b).iter().any(|(name, _)| name == n);
        assert!(has(&with, b"EXIF") && has(&with, b"VP8X"));
        let kept = optimise(
            &with,
            &OptimiseOptions {
                strip_metadata: false,
                ..OptimiseOptions::default()
            },
        )
        .unwrap();
        assert!(has(&kept, b"EXIF"));
        let stripped = optimise(&with, &OptimiseOptions::default()).unwrap();
        assert!(!has(&stripped, b"EXIF"));
    }

    #[test]
    fn a_huge_animation_is_refused_before_decoding() {
        let mut bytes = testutil::animated_webp_frames(&[100, 100], 16, 16);
        // VP8X canvas 16 383 x 16 383: width - 1 and height - 1 as 24-bit LE at 24 and 27.
        bytes[24..27].copy_from_slice(&[0xFE, 0x3F, 0]);
        bytes[27..30].copy_from_slice(&[0xFE, 0x3F, 0]);
        assert!(
            matches!(decode(&bytes), Err(OptimiseError::Unsupported(m)) if m == t("pdf.anim.too_large"))
        );
    }

    #[test]
    fn a_damaged_animation_is_an_error_not_a_hang() {
        let bytes = testutil::animated_webp_frames(&[100, 100, 100], 64, 48);
        let cut = &bytes[..bytes.len() * 2 / 3];
        // Either the demuxer refuses it or a frame fails to decode.
        assert!(decode(cut).is_err() || decode(cut).unwrap().frames.len() < 3);
    }

    #[test]
    fn the_first_frame_comes_with_its_profile() {
        let r = first_frame(&testutil::animated_webp_frames(&[100, 100], 64, 48)).unwrap();
        assert_eq!((r.width, r.height, r.rgba.len()), (64, 48, 64 * 48 * 4));
    }
}
