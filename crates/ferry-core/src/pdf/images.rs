//! The images of a PDF (sE pdf-formats): how sharp each one is drawn, and
//! recompressing them, downsampled to a DPI. Content streams are only read,
//! never rewritten, so text, links and forms stay as they are.

use std::collections::HashMap;
use std::io::Write;
use std::sync::mpsc;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use rayon::prelude::*;

use super::{MAX_DECOMPRESSED, inherited};
use crate::edit::geometry::Rect;
use crate::edit::raster::{self, Raster};
use crate::optimise::png::MAX_PIXELS;
use crate::optimise::{OptimiseError, OptimiseOptions, jpeg};

/// An image is resampled only when its DPI is above `target` times this.
pub const DOWNSAMPLE_SLACK: f32 = 1.1;

/// Colours a sample must hold to count as a photograph.
const PHOTO_COLOURS: usize = 1024;
/// Pictures this big are recompressed one at a time, so a few of them never
/// fill the memory between them.
const BIG_PIXELS: u64 = 16_000_000;
const MAX_FORM_DEPTH: usize = 12;
/// Operators read across the whole file, so a hostile one cannot stall us.
const MAX_OPERATIONS: usize = 5_000_000;

/// `[a, b, c, d, e, f]` of a PDF matrix.
type Matrix = [f32; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// PDF's `cm` rule: the new CTM is `m × ctm`.
pub fn concat(m: Matrix, c: Matrix) -> Matrix {
    [
        m[0] * c[0] + m[1] * c[2],
        m[0] * c[1] + m[1] * c[3],
        m[2] * c[0] + m[3] * c[2],
        m[2] * c[1] + m[3] * c[3],
        m[4] * c[0] + m[5] * c[2] + c[4],
        m[4] * c[1] + m[5] * c[3] + c[5],
    ]
}

fn matrix(operands: &[Object]) -> Option<Matrix> {
    let numbers: Vec<f32> = operands
        .iter()
        .map(|o| o.as_float().ok())
        .collect::<Option<_>>()?;
    numbers.try_into().ok()
}

/// The XObject `name` of a resource dictionary.
fn xobject<'a>(
    doc: &'a Document,
    resources: &'a Dictionary,
    name: &[u8],
) -> Option<(ObjectId, &'a Stream)> {
    let xobjects = doc.dereference(resources.get(b"XObject").ok()?).ok()?.1;
    let id = xobjects
        .as_dict()
        .ok()?
        .get(name)
        .ok()?
        .as_reference()
        .ok()?;
    Some((id, doc.get_object(id).ok()?.as_stream().ok()?))
}

fn subtype(stream: &Stream) -> Option<&[u8]> {
    stream.dict.get(b"Subtype").ok()?.as_name().ok()
}

fn pixel_size(stream: &Stream) -> Option<(u32, u32)> {
    let side = |key: &[u8]| u32::try_from(stream.dict.get(key).ok()?.as_i64().ok()?).ok();
    Some((side(b"Width")?, side(b"Height")?)).filter(|&(w, h)| w > 0 && h > 0)
}

struct Walker<'a> {
    doc: &'a Document,
    dpis: HashMap<ObjectId, f32>,
    budget: usize,
    /// The forms being walked, so a form that draws itself stops.
    chain: Vec<ObjectId>,
}

impl<'a> Walker<'a> {
    fn run(&mut self, bytes: &[u8], resources: &'a Dictionary, start: Matrix, depth: usize) {
        let Ok(content) = Content::decode(bytes) else {
            return;
        };
        let (mut ctm, mut stack) = (start, Vec::new());
        for op in &content.operations {
            if self.budget == 0 {
                return;
            }
            self.budget -= 1;
            match op.operator.as_str() {
                "q" => stack.push(ctm),
                "Q" => {
                    if let Some(saved) = stack.pop() {
                        ctm = saved;
                    }
                }
                "cm" => {
                    if let Some(m) = matrix(&op.operands) {
                        ctm = concat(m, ctm);
                    }
                }
                "Do" => {
                    if let Some(name) = op.operands.first().and_then(|o| o.as_name().ok()) {
                        self.draw(name, resources, ctm, depth);
                    }
                }
                _ => {}
            }
        }
    }

    fn draw(&mut self, name: &[u8], resources: &'a Dictionary, ctm: Matrix, depth: usize) {
        let Some((id, stream)) = xobject(self.doc, resources, name) else {
            return;
        };
        match subtype(stream) {
            Some(b"Image") => {
                let Some((w, h)) = pixel_size(stream) else {
                    return;
                };
                let across = ctm[0].hypot(ctm[1]);
                let down = ctm[2].hypot(ctm[3]);
                if across < 0.01 || down < 0.01 {
                    return;
                }
                let dpi = (w as f32 * 72.0 / across).min(h as f32 * 72.0 / down);
                let seen = self.dpis.entry(id).or_insert(dpi);
                *seen = seen.min(dpi);
            }
            Some(b"Form") if depth < MAX_FORM_DEPTH && !self.chain.contains(&id) => {
                let own = stream
                    .dict
                    .get(b"Resources")
                    .ok()
                    .and_then(|r| self.doc.dereference(r).ok())
                    .and_then(|(_, r)| r.as_dict().ok());
                let form = stream
                    .dict
                    .get(b"Matrix")
                    .ok()
                    .and_then(|m| m.as_array().ok())
                    .and_then(|m| matrix(m))
                    .unwrap_or(IDENTITY);
                let Ok(bytes) = stream.decompressed_content_with_limit(MAX_DECOMPRESSED) else {
                    return;
                };
                self.chain.push(id);
                self.run(
                    &bytes,
                    own.unwrap_or(resources),
                    concat(form, ctm),
                    depth + 1,
                );
                self.chain.pop();
            }
            _ => {}
        }
    }
}

/// For each image drawn, the sharpest-needed resolution: pixels per inch of
/// its largest drawing. Inline images and images never drawn are absent.
pub fn image_dpis(doc: &Document) -> HashMap<ObjectId, f32> {
    let mut walker = Walker {
        doc,
        dpis: HashMap::new(),
        budget: MAX_OPERATIONS,
        chain: Vec::new(),
    };
    for page in doc.page_iter() {
        let Ok(content) = doc.get_page_content_with_limit(page, MAX_DECOMPRESSED) else {
            continue;
        };
        if let Some(resources) = inherited(doc, page, b"Resources").and_then(|r| r.as_dict().ok()) {
            walker.run(&content, resources, IDENTITY, 0);
        }
    }
    walker.dpis
}

/// Whether a sample of `pixels` (`channels` bytes each) holds the many colours
/// of a photograph rather than the few of a screenshot or a drawing.
pub fn photo_like(pixels: &[u8], channels: usize) -> bool {
    let count = pixels.len() / channels;
    let stride = (count / 65_536).max(1);
    let mut seen = std::collections::HashSet::new();
    for pixel in pixels.chunks_exact(channels).step_by(stride).take(65_536) {
        seen.insert(pixel.iter().fold(0_u32, |all, &c| all << 8 | u32::from(c)));
        if seen.len() >= PHOTO_COLOURS {
            return true;
        }
    }
    false
}

/// An image XObject Ferry can recompress.
#[derive(Clone, Copy)]
struct Candidate {
    px: (u32, u32),
    channels: usize,
    dct: bool,
    has_smask: bool,
}

fn candidate(doc: &Document, stream: &Stream) -> Option<Candidate> {
    let dict = &stream.dict;
    if subtype(stream)? != b"Image"
        || dict.get(b"BitsPerComponent").ok()?.as_i64().ok()? != 8
        || dict
            .get(b"ImageMask")
            .is_ok_and(|m| m.as_bool().unwrap_or(false))
        || dict.has(b"Mask")
        || dict.has(b"Decode")
    {
        return None;
    }
    let px = pixel_size(stream).filter(|&(w, h)| u64::from(w) * u64::from(h) <= MAX_PIXELS)?;
    let has_smask = match dict.get(b"SMask") {
        Ok(smask) => {
            let smask = doc.dereference(smask).ok()?.1.as_stream().ok()?;
            if smask.dict.has(b"Matte") {
                return None;
            }
            true
        }
        Err(_) => false,
    };
    let channels = match doc.dereference(dict.get(b"ColorSpace").ok()?).ok()?.1 {
        Object::Name(name) if name == b"DeviceRGB" => 3,
        Object::Name(name) if name == b"DeviceGray" => 1,
        Object::Array(parts) if parts.first()?.as_name().ok()? == b"ICCBased" => {
            let profile = doc.dereference(parts.get(1)?).ok()?.1.as_stream().ok()?;
            match profile.dict.get(b"N").ok()?.as_i64().ok()? {
                3 => 3,
                1 => 1,
                _ => return None,
            }
        }
        _ => return None,
    };
    let dct = match filters(stream)?.as_slice() {
        [] => false,
        [f] if f == b"FlateDecode" => false,
        [f] if f == b"DCTDecode" => true,
        _ => return None,
    };
    Some(Candidate {
        px,
        channels,
        dct,
        has_smask,
    })
}

/// The stream's filters (a name or an array of names).
fn filters(stream: &Stream) -> Option<Vec<Vec<u8>>> {
    match stream.dict.get(b"Filter") {
        Err(_) => Some(Vec::new()),
        Ok(Object::Name(name)) => Some(vec![name.clone()]),
        Ok(Object::Array(names)) => names
            .iter()
            .map(|n| n.as_name().ok().map(<[u8]>::to_vec))
            .collect(),
        Ok(_) => None,
    }
}

struct Replacement {
    image: Stream,
    smask: Option<Stream>,
}

/// Recompress every image that can be, in parallel, downsampling the ones
/// drawn sharper than `target_dpi`. Returns how many were replaced.
pub fn recompress_all(
    doc: &mut Document,
    target_dpi: u16,
    o: &OptimiseOptions,
    progress: &mut dyn FnMut(f32),
) -> Result<usize, OptimiseError> {
    let dpis = image_dpis(doc);
    let mut jobs: Vec<(ObjectId, Candidate)> = doc
        .objects
        .iter()
        .filter_map(|(&id, object)| Some((id, candidate(doc, object.as_stream().ok()?)?)))
        .collect();
    let total = jobs.len();
    // Big pictures last, one at a time.
    jobs.sort_by_key(|(_, c)| u64::from(c.px.0) * u64::from(c.px.1) > BIG_PIXELS);
    let split = jobs.partition_point(|(_, c)| u64::from(c.px.0) * u64::from(c.px.1) <= BIG_PIXELS);
    let (small, big) = jobs.split_at(split);

    let shared: &Document = doc;
    let one = |&(id, c): &(ObjectId, Candidate)| -> Result<Option<(ObjectId, Replacement)>, OptimiseError> {
        if o.cancel.is_cancelled() {
            return Err(OptimiseError::Cancelled);
        }
        let dpi = dpis.get(&id).copied();
        Ok(recompress(shared, id, c, dpi, target_dpi, o).map(|r| (id, r)))
    };
    let mut done = Vec::new();
    let mut cancelled = false;
    let mut finished = 0;
    let mut note = |result: Result<Option<(ObjectId, Replacement)>, OptimiseError>,
                    progress: &mut dyn FnMut(f32)| {
        finished += 1;
        progress(finished as f32 / total as f32);
        match result {
            Ok(Some(replacement)) => done.push(replacement),
            Ok(None) => {}
            Err(_) => cancelled = true,
        }
    };
    let (sender, receiver) = mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            small
                .par_iter()
                .for_each_with(sender, |sender, job| drop(sender.send(one(job))));
        });
        for result in receiver {
            note(result, progress);
        }
    });
    for job in big {
        note(one(job), progress);
    }
    if cancelled {
        return Err(OptimiseError::Cancelled);
    }
    done.sort_by_key(|(id, _)| *id);
    let replaced = done.len();
    for (id, Replacement { mut image, smask }) in done {
        if let Some(smask) = smask {
            let smask = doc.add_object(smask);
            image.dict.set("SMask", smask);
        }
        doc.objects.insert(id, Object::Stream(image));
    }
    Ok(replaced)
}

fn scaled(side: u32, f: f32) -> u32 {
    ((side as f32 * f).round() as u32).max(1)
}

/// The image as RGBA (gray replicated), whatever its filter.
fn decode(stream: &Stream, c: Candidate) -> Option<Raster> {
    let (w, h) = c.px;
    if c.dct {
        let (r, _) = jpeg::decode_raster(&stream.content).ok()?;
        return ((r.width, r.height) == c.px).then_some(r);
    }
    let data = if stream.dict.has(b"Filter") {
        stream
            .decompressed_content_with_limit(MAX_DECOMPRESSED)
            .ok()?
    } else {
        stream.content.clone()
    };
    let need = w as usize * h as usize * c.channels;
    let data = data.get(..need)?;
    let rgba = if c.channels == 3 {
        data.as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect()
    } else {
        data.iter().flat_map(|&v| [v, v, v, 255]).collect()
    };
    Some(Raster {
        width: w,
        height: h,
        rgba,
        icc: None,
    })
}

/// Interleaved samples of an opaque RGBA picture: three bytes a pixel, or one.
fn samples(r: &Raster, channels: usize) -> Vec<u8> {
    let pixels = r.rgba.as_chunks::<4>().0.iter();
    if channels == 3 {
        pixels.flat_map(|p| [p[0], p[1], p[2]]).collect()
    } else {
        pixels.map(|p| p[0]).collect()
    }
}

fn deflate(data: &[u8]) -> Option<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(data).ok()?;
    encoder.finish().ok()
}

fn recompress(
    doc: &Document,
    id: ObjectId,
    c: Candidate,
    dpi: Option<f32>,
    target_dpi: u16,
    o: &OptimiseOptions,
) -> Option<Replacement> {
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;
    let f = match dpi {
        Some(d) if d > f32::from(target_dpi) * DOWNSAMPLE_SLACK => f32::from(target_dpi) / d,
        _ => 1.0,
    };
    let (w, h) = c.px;
    let size = if f < 1.0 {
        (scaled(w, f), scaled(h, f))
    } else {
        c.px
    };
    let whole = decode(stream, c)?;
    let picture = if size == c.px {
        whole
    } else {
        let all = Rect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        };
        raster::cut(&whole, all, size).ok()?
    };
    let pixels = samples(&picture, c.channels);
    let (content, filter) = if c.dct || photo_like(&pixels, c.channels) {
        let jpeg = if c.channels == 1 {
            jpeg::encode_gray(size.0, size.1, &pixels, o)
        } else {
            jpeg::encode_raster(&picture, o)
        };
        (jpeg.ok()?, "DCTDecode")
    } else {
        (deflate(&pixels)?, "FlateDecode")
    };
    let mut dict = stream.dict.clone();
    dict.set("Width", i64::from(size.0));
    dict.set("Height", i64::from(size.1));
    dict.set("BitsPerComponent", 8);
    dict.set("Filter", filter);
    dict.remove(b"DecodeParms");
    let mut before = stream.content.len();
    let mut after = content.len();
    let image = Stream::new(dict, content);
    // The soft mask follows the picture to its new size.
    let mut smask = None;
    if c.has_smask && size != c.px {
        let old = doc
            .dereference(stream.dict.get(b"SMask").ok()?)
            .ok()?
            .1
            .as_stream()
            .ok()?;
        if let Some(resized) = resize_mask(old, size) {
            before += old.content.len();
            after += resized.content.len();
            smask = Some(resized);
        }
    }
    (after < before).then_some(Replacement { image, smask })
}

/// A gray 8-bit soft mask resized to `size`, deflated; `None` when it is
/// anything else.
fn resize_mask(mask: &Stream, size: (u32, u32)) -> Option<Stream> {
    let px = pixel_size(mask)?;
    let plain = mask.dict.get(b"BitsPerComponent").ok()?.as_i64().ok()? == 8
        && mask.dict.get(b"ColorSpace").ok()?.as_name().ok()? == b"DeviceGray"
        && matches!(filters(mask)?.as_slice(), [] | [_])
        && u64::from(px.0) * u64::from(px.1) <= MAX_PIXELS;
    if !plain {
        return None;
    }
    let candidate = Candidate {
        px,
        channels: 1,
        dct: false,
        has_smask: false,
    };
    let whole = decode(mask, candidate)?;
    let all = Rect {
        x: 0,
        y: 0,
        width: px.0,
        height: px.1,
    };
    let small = raster::cut(&whole, all, size).ok()?;
    let mut dict = Dictionary::new();
    dict.set("Type", "XObject");
    dict.set("Subtype", "Image");
    dict.set("Width", i64::from(size.0));
    dict.set("Height", i64::from(size.1));
    dict.set("ColorSpace", "DeviceGray");
    dict.set("BitsPerComponent", 8);
    dict.set("Filter", "FlateDecode");
    Some(Stream::new(dict, deflate(&samples(&small, 1))?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{self, ImageKind, PdfExtras, PdfImage, PdfPage};

    /// One page with `kind` drawn over all of it at `dpi`.
    fn scan_at(w: u32, h: u32, dpi: f32, kind: ImageKind) -> Vec<u8> {
        let drawn = (w as f32 * 72.0 / dpi, h as f32 * 72.0 / dpi);
        let page = PdfPage {
            media: [0.0, 0.0, drawn.0, drawn.1],
            images: vec![PdfImage {
                px: (w, h),
                drawn,
                kind,
            }],
            ..PdfPage::default()
        };
        testutil::pdf(&[page], &PdfExtras::default())
    }

    /// A 300 dpi scan.
    fn scan(w: u32, h: u32, kind: ImageKind) -> Vec<u8> {
        scan_at(w, h, 300.0, kind)
    }

    /// The image XObjects of a PDF, in object order.
    fn images(pdf: &[u8]) -> Vec<Stream> {
        let doc = Document::load_mem(pdf).unwrap();
        doc.objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .filter(|s| subtype(s) == Some(b"Image"))
            .cloned()
            .collect()
    }

    fn width_of(pdf: &[u8], n: usize) -> i64 {
        images(pdf)[n].dict.get(b"Width").unwrap().as_i64().unwrap()
    }

    fn name_of(pdf: &[u8], n: usize, key: &[u8]) -> Vec<u8> {
        images(pdf)[n]
            .dict
            .get(key)
            .unwrap()
            .as_name()
            .unwrap()
            .to_vec()
    }

    fn filter_of(pdf: &[u8], n: usize) -> Vec<u8> {
        name_of(pdf, n, b"Filter")
    }

    fn colour_space_of(pdf: &[u8], n: usize) -> Vec<u8> {
        name_of(pdf, n, b"ColorSpace")
    }

    fn image_bytes(pdf: &[u8], n: usize) -> Vec<u8> {
        images(pdf)[n].content.clone()
    }

    fn optimise(pdf: &[u8], dpi: u16) -> Vec<u8> {
        crate::pdf::optimise(pdf, dpi, &OptimiseOptions::default(), &mut |_| {}).unwrap()
    }

    #[test]
    fn concat_scales_and_nests() {
        assert_eq!(
            concat([720.0, 0.0, 0.0, 360.0, 0.0, 0.0], IDENTITY),
            [720.0, 0.0, 0.0, 360.0, 0.0, 0.0]
        );
        assert_eq!(
            concat(
                [720.0, 0.0, 0.0, 360.0, 10.0, 0.0],
                [0.5, 0.0, 0.0, 0.5, 0.0, 0.0]
            ),
            [360.0, 0.0, 0.0, 180.0, 5.0, 0.0]
        );
    }

    #[test]
    fn an_image_drawn_across_four_inches_is_three_hundred_dpi() {
        let image = PdfImage {
            px: (1200, 600),
            drawn: (288.0, 144.0),
            kind: ImageKind::DctRgb,
        };
        let page = PdfPage {
            images: vec![image],
            ..PdfPage::default()
        };
        let pdf = testutil::pdf(&[page], &PdfExtras::default());
        let doc = Document::load_mem(&pdf).unwrap();
        assert_eq!(
            image_dpis(&doc).values().copied().collect::<Vec<_>>(),
            [300.0]
        );
    }

    #[test]
    fn a_shared_image_keeps_the_dpi_of_its_largest_use() {
        // Page 1 draws the image 288 pt wide (300 dpi), page 2 the same object half that.
        let image = PdfImage {
            px: (1200, 600),
            drawn: (288.0, 144.0),
            kind: ImageKind::DctRgb,
        };
        let first = PdfPage {
            images: vec![image],
            ..PdfPage::default()
        };
        let extras = PdfExtras {
            shared_image: true,
            ..Default::default()
        };
        let pdf = testutil::pdf(&[first, PdfPage::default()], &extras);
        let doc = Document::load_mem(&pdf).unwrap();
        assert_eq!(
            image_dpis(&doc).values().copied().collect::<Vec<_>>(),
            [300.0]
        );
        assert_eq!(width_of(&optimise(&pdf, 150), 0), 600);
    }

    #[test]
    fn a_300_dpi_scan_comes_down_to_150() {
        let pdf = scan(1200, 1500, ImageKind::DctRgb);
        let out = optimise(&pdf, 150);
        assert!((599..=601).contains(&width_of(&out, 0)));
        assert!(out.len() < pdf.len());
    }

    #[test]
    fn an_image_just_above_the_target_keeps_its_size() {
        // 160 dpi, under 150 x 1.1.
        let out = optimise(&scan_at(640, 800, 160.0, ImageKind::DctRgb), 150);
        assert_eq!(width_of(&out, 0), 640);
    }

    #[test]
    fn a_screenshot_stays_lossless_and_a_photo_becomes_jpeg() {
        let shot = optimise(&scan(1200, 1500, ImageKind::FlateScreenshot), 150);
        assert_eq!(filter_of(&shot, 0), b"FlateDecode");
        let photo = optimise(&scan(1200, 1500, ImageKind::FlatePhoto), 150);
        assert_eq!(filter_of(&photo, 0), b"DCTDecode");
    }

    #[test]
    fn gray_stays_gray() {
        let out = optimise(&scan(1200, 1500, ImageKind::DctGray), 150);
        assert_eq!(colour_space_of(&out, 0), b"DeviceGray");
    }

    #[test]
    fn images_the_engine_cannot_handle_are_left_alone() {
        for kind in [ImageKind::DctCmyk, ImageKind::OneBit, ImageKind::Jpx] {
            let pdf = scan(1200, 1500, kind);
            let out = optimise(&pdf, 72);
            assert_eq!(image_bytes(&out, 0), image_bytes(&pdf, 0), "{kind:?}");
        }
    }

    #[test]
    fn photo_like_counts_colours() {
        assert!(!photo_like(&[10, 20, 30].repeat(10_000), 3));
        let gradient: Vec<u8> = (0..90_000_u32)
            .flat_map(|i| [(i % 251) as u8, (i / 251 % 251) as u8, (i % 7) as u8])
            .collect();
        assert!(photo_like(&gradient, 3));
    }

    #[test]
    fn a_cancel_stops_recompressing() {
        let o = OptimiseOptions::default();
        o.cancel.cancel();
        let pdf = scan(1200, 1500, ImageKind::DctRgb);
        assert!(matches!(
            crate::pdf::optimise(&pdf, 150, &o, &mut |_| {}),
            Err(OptimiseError::Cancelled)
        ));
    }

    #[test]
    fn a_soft_mask_follows_the_picture() {
        let mut dict = Dictionary::new();
        dict.set("Width", 8);
        dict.set("Height", 8);
        dict.set("ColorSpace", "DeviceGray");
        dict.set("BitsPerComponent", 8);
        let mask = Stream::new(dict, vec![200; 64]);
        let small = resize_mask(&mask, (4, 4)).unwrap();
        assert_eq!(pixel_size(&small), Some((4, 4)));
        assert_eq!(small.decompressed_content().unwrap(), vec![200; 16]);
    }
}
