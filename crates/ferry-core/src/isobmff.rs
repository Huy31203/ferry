//! HEIF and AVIF files read from their ISO base media boxes alone: the `ftyp`
//! brands say which one it is, and the primary picture's size sits in
//! `meta` → `iprp` → `ipco`.

use crate::sniff::Format;

/// Brands of HEIF images (HEIC, and HEVC image sequences).
const HEIC_BRANDS: [&[u8; 4]; 8] = [
    b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"mif1", b"msf1",
];

/// Major brands of image sequences.
const SEQUENCE_BRANDS: [&[u8; 4]; 4] = [b"avis", b"msf1", b"hevc", b"hevx"];

/// The boxes in `bytes`, as `(type, payload)`. A truncated last box is cut at
/// the end of the data; a box that cannot hold its own header ends the walk.
fn boxes(bytes: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    let mut rest = bytes;
    std::iter::from_fn(move || {
        let head = rest.get(..8)?;
        let size = u32::from_be_bytes(head[..4].try_into().ok()?) as usize;
        let kind: [u8; 4] = head[4..8].try_into().ok()?;
        let (header, end) = match size {
            0 => (8, rest.len()),
            1 => {
                let big = u64::from_be_bytes(rest.get(8..16)?.try_into().ok()?);
                (16, usize::try_from(big).ok()?)
            }
            n => (8, n),
        };
        if end < header {
            return None;
        }
        let payload = rest.get(header..end.min(rest.len()))?;
        rest = rest.get(end.min(rest.len())..)?;
        Some((kind, payload))
    })
}

fn child<'a>(bytes: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(bytes).find(|(k, _)| k == kind).map(|(_, p)| p)
}

/// The major brand and the compatible brands of a file that starts with `ftyp`.
pub fn brands(bytes: &[u8]) -> Option<([u8; 4], Vec<[u8; 4]>)> {
    let (kind, payload) = boxes(bytes).next()?;
    if &kind != b"ftyp" {
        return None;
    }
    let major: [u8; 4] = payload.get(..4)?.try_into().ok()?;
    let compatible = payload
        .get(8..)
        .unwrap_or_default()
        .as_chunks::<4>()
        .0
        .to_vec();
    Some((major, compatible))
}

/// `Avif` for any `avif`/`avis` brand, else `Heic` for a HEIF image brand;
/// `None` for everything else (an MP4 is not an image).
pub fn image_format(bytes: &[u8]) -> Option<Format> {
    let (major, compatible) = brands(bytes)?;
    let any = |wanted: &[&[u8; 4]]| {
        std::iter::once(&major)
            .chain(&compatible)
            .any(|b| wanted.contains(&b))
    };
    if any(&[b"avif", b"avis"]) {
        Some(Format::Avif)
    } else if any(&HEIC_BRANDS) {
        Some(Format::Heic)
    } else {
        None
    }
}

/// An image sequence (burst, animation) rather than a still.
pub fn is_sequence(bytes: &[u8]) -> bool {
    brands(bytes).is_some_and(|(major, _)| SEQUENCE_BRANDS.contains(&&major))
}

/// The primary picture's size: the largest `ispe` in `meta` → `iprp` → `ipco`
/// (thumbnails come smaller), with the sides swapped by a quarter-turn `irot`.
pub fn primary_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let meta = boxes(bytes).find(|(k, _)| k == b"meta")?.1.get(4..)?; // a full box
    let ipco = child(child(meta, b"iprp")?, b"ipco")?;
    let (w, h) = boxes(ipco)
        .filter(|(k, _)| k == b"ispe")
        .filter_map(|(_, p)| {
            let w = u32::from_be_bytes(p.get(4..8)?.try_into().ok()?);
            let h = u32::from_be_bytes(p.get(8..12)?.try_into().ok()?);
            Some((w, h))
        })
        .max_by_key(|&(w, h)| u64::from(w) * u64::from(h))?;
    let turned = child(ipco, b"irot")
        .and_then(|p| p.first())
        .is_some_and(|a| a & 3 == 1 || a & 3 == 3);
    Some(if turned { (h, w) } else { (w, h) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn the_largest_ispe_is_the_primary_size_and_irot_turns_it() {
        let thumb_and_full = testutil::heif(b"heic", &[(8, 6), (64, 48)], None);
        assert_eq!(primary_size(&thumb_and_full), Some((64, 48)));
        assert_eq!(
            primary_size(&testutil::heif(b"heic", &[(64, 48)], Some(1))),
            Some((48, 64))
        );
        assert_eq!(primary_size(&testutil::ftyp(b"heic", &[])), None);
    }

    #[test]
    fn brands_are_read_from_the_first_box() {
        let (major, compatible) = brands(&testutil::ftyp(b"heic", &[b"mif1", b"heic"])).unwrap();
        assert_eq!((&major, compatible.len()), (b"heic", 2));
        assert!(brands(b"not a box at all").is_none());
    }

    #[test]
    fn only_image_brands_are_images() {
        assert_eq!(
            image_format(&testutil::ftyp(b"mif1", &[b"avif"])),
            Some(Format::Avif)
        );
        assert_eq!(
            image_format(&testutil::ftyp(b"heix", &[])),
            Some(Format::Heic)
        );
        assert_eq!(image_format(&testutil::ftyp(b"isom", &[b"mp41"])), None);
    }

    #[test]
    fn a_cut_short_file_is_not_a_panic() {
        let full = testutil::heif(b"heic", &[(64, 48)], Some(1));
        for len in 0..full.len() {
            let _ = primary_size(&full[..len]);
            let _ = image_format(&full[..len]);
        }
    }
}
