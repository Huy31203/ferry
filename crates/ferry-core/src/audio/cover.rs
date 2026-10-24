//! Cover art: pulling the picture out of a file and processing it with Ferry's
//! own image engine (ffmpeg only copies it).

use std::{ffi::OsString, path::Path};

use super::plan::CoverPlan;
use crate::{
    edit::{self, Edits, Framing, raster},
    optimise::{self, Level, OptimiseError, OptimiseOptions, Outcome},
    sniff::{self, Format},
};

/// The extension of a picture ffmpeg copies out as is; other codecs are not used.
pub fn file_ext(codec: &str) -> Option<&'static str> {
    match codec {
        "mjpeg" => Some("jpg"),
        "png" => Some("png"),
        _ => None,
    }
}

/// Copy the first picture to `out`; `exec` adds the global flags.
pub fn extract_args(input: &Path, out: &Path) -> Vec<OsString> {
    let mut args = vec![OsString::from("-i"), input.as_os_str().to_owned()];
    args.extend(
        [
            "-map",
            "0:v:0",
            "-c",
            "copy",
            "-frames:v",
            "1",
            "-f",
            "image2",
        ]
        .map(OsString::from),
    );
    args.push(out.as_os_str().to_owned());
    args
}

#[derive(Debug)]
pub struct Processed {
    pub bytes: Vec<u8>,
    pub ext: &'static str,
    pub dims: (u32, u32),
}

/// `Optimise` keeps the picture's format; `Resize` and `Scale` give a JPEG at
/// quality 85 and never enlarge. Anything else is not a picture job.
pub fn process(
    bytes: &[u8],
    plan: CoverPlan,
    o: &OptimiseOptions,
) -> Result<Processed, OptimiseError> {
    let format = sniff::sniff(bytes)
        .filter(|f| matches!(f, Format::Png | Format::Jpeg))
        .ok_or_else(|| OptimiseError::Unsupported("not a PNG or JPEG cover".into()))?;
    let o = OptimiseOptions {
        level: Level::Normal,
        ..o.clone()
    };
    let (bytes, format) = match plan {
        CoverPlan::Optimise => match optimise::optimise(bytes, format, &o)? {
            Outcome::Smaller { bytes } => (bytes, format),
            _ => (bytes.to_vec(), format),
        },
        CoverPlan::Resize(_) | CoverPlan::Scale(_) => {
            let mut edits = Edits {
                framing: Framing::Centre,
                format: Some(Format::Jpeg),
                compression: Some(85),
                ..Edits::default()
            };
            match plan {
                CoverPlan::Resize(size) => edits.size = Some(size),
                CoverPlan::Scale(percent) => edits.scale = Some(percent),
                _ => unreachable!(),
            }
            let applied = edit::apply(bytes, format, &edits, &o)?;
            (applied.bytes, applied.format)
        }
        _ => return Err(OptimiseError::Unsupported("not a cover job".into())),
    };
    Ok(Processed {
        dims: raster::display_dims(&bytes, format).unwrap_or_default(),
        ext: if format == Format::Png { "png" } else { "jpg" },
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{edit::Size, sniff::sniff, testutil};

    #[test]
    fn optimise_keeps_the_size_and_format_and_shrinks() {
        let src = testutil::jpeg(1200, 1200, 95, &Default::default());
        let p = process(&src, CoverPlan::Optimise, &OptimiseOptions::default()).unwrap();
        assert_eq!((p.ext, p.dims), ("jpg", (1200, 1200)));
        assert!(p.bytes.len() < src.len());
    }

    #[test]
    fn downscale_squares_or_scales_and_never_enlarges() {
        let o = OptimiseOptions::default();
        let sq = process(
            &testutil::jpeg(1600, 1200, 90, &Default::default()),
            CoverPlan::Resize(Size::Exact {
                width: 600,
                height: 600,
            }),
            &o,
        )
        .unwrap();
        assert_eq!((sq.ext, sq.dims), ("jpg", (600, 600)));
        let half = process(&testutil::png(1200, 800), CoverPlan::Scale(50), &o).unwrap();
        assert_eq!(
            (half.ext, half.dims, sniff(&half.bytes)),
            ("jpg", (600, 400), Some(Format::Jpeg))
        );
        let big = process(
            &testutil::png(500, 500),
            CoverPlan::Resize(Size::LongEdge { edge: 3000 }),
            &o,
        )
        .unwrap();
        assert_eq!(big.dims, (500, 500));
    }

    #[test]
    fn unreadable_covers_are_an_error() {
        assert!(
            process(
                b"BM not a jpeg",
                CoverPlan::Optimise,
                &OptimiseOptions::default()
            )
            .is_err()
        );
        assert_eq!(
            (file_ext("mjpeg"), file_ext("png"), file_ext("bmp")),
            (Some("jpg"), Some("png"), None)
        );
    }

    #[test]
    fn extraction_copies_the_first_picture() {
        let a: Vec<String> =
            extract_args(Path::new("/b/a.mp3"), Path::new("/w/cover-original.jpg"))
                .iter()
                .map(|s| s.to_string_lossy().into())
                .collect();
        assert_eq!(
            a,
            [
                "-i",
                "/b/a.mp3",
                "-map",
                "0:v:0",
                "-c",
                "copy",
                "-frames:v",
                "1",
                "-f",
                "image2",
                "/w/cover-original.jpg"
            ]
        );
    }
}
