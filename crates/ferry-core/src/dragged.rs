//! Images dragged from another app with no file behind them (image bytes on the
//! drag pasteboard): named like clipboard images and saved as real files.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;

use crate::i18n::{t, tf};
use crate::output::unique;
use crate::template::safe_file_name;

/// One dragged image's bytes and the extension they are saved under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraggedImage {
    pub ext: &'static str,
    pub bytes: Vec<u8>,
}

/// What `save_dragged` did: the files written, the items with no readable image, and
/// whether the folder could not be created or written.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Saved {
    pub paths: Vec<PathBuf>,
    pub unreadable: u32,
    pub cant_save: bool,
}

/// "Slack image 2026-11-09 at 15.42.10": the stem of a dragged image's file.
pub fn dragged_name(source: &str, now: NaiveDateTime) -> String {
    let source = if source.trim().is_empty() {
        t("dragged.source_unknown")
    } else {
        source.to_string()
    };
    let date = now.format("%Y-%m-%d").to_string();
    let time = now.format("%H.%M.%S").to_string();
    safe_file_name(&tf(
        "dragged.name",
        &[("source", &source), ("date", &date), ("time", &time)],
    ))
}

/// Write each readable item into `folder` (created if missing). `None` items count as unreadable.
pub fn save_dragged(
    folder: &Path,
    source: &str,
    items: Vec<Option<DraggedImage>>,
    now: NaiveDateTime,
) -> Saved {
    let mut saved = Saved::default();
    if fs::create_dir_all(folder).is_err() {
        saved.cant_save = true;
        return saved;
    }
    let stem = dragged_name(source, now);
    for item in items {
        let Some(image) = item else {
            saved.unreadable += 1;
            continue;
        };
        let path = unique(&folder.join(format!("{stem}.{}", image.ext)), None);
        match fs::write(&path, &image.bytes) {
            Ok(()) => saved.paths.push(path),
            Err(_) => saved.cant_save = true,
        }
    }
    saved
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 9)
            .unwrap()
            .and_hms_opt(15, 42, 10)
            .unwrap()
    }

    fn png() -> Option<DraggedImage> {
        Some(DraggedImage {
            ext: "png",
            bytes: b"\x89PNG fake".to_vec(),
        })
    }

    #[test]
    fn a_dragged_image_is_named_after_its_app() {
        assert_eq!(dragged_name("Slack", at()), "Slack image 2026-11-09 at 15.42.10");
    }

    #[test]
    fn an_awkward_source_name_is_made_safe() {
        assert_eq!(dragged_name("A/B: C", at()), "A_B_ C image 2026-11-09 at 15.42.10");
        assert_eq!(dragged_name("", at()), "Dragged image 2026-11-09 at 15.42.10");
    }

    #[test]
    fn bytes_are_written_unchanged_into_a_new_folder() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Ferry");
        let saved = save_dragged(&folder, "Slack", vec![png()], at());
        assert_eq!(
            saved.paths,
            vec![folder.join("Slack image 2026-11-09 at 15.42.10.png")]
        );
        assert_eq!(std::fs::read(&saved.paths[0]).unwrap(), b"\x89PNG fake");
        assert_eq!((saved.unreadable, saved.cant_save), (0, false));
    }

    #[test]
    fn two_images_in_one_second_get_distinct_names() {
        let dir = tempfile::tempdir().unwrap();
        let saved = save_dragged(dir.path(), "Slack", vec![png(), png()], at());
        assert_eq!(saved.paths.len(), 2);
        assert_ne!(saved.paths[0], saved.paths[1]);
        assert!(saved.paths.iter().all(|p| p.exists()));
    }

    #[test]
    fn unreadable_items_are_counted_and_the_rest_saved() {
        let dir = tempfile::tempdir().unwrap();
        let saved = save_dragged(dir.path(), "Slack", vec![None, png()], at());
        assert_eq!((saved.paths.len(), saved.unreadable), (1, 1));
    }

    #[test]
    fn an_unwritable_folder_saves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, b"x").unwrap();
        let saved = save_dragged(&blocker.join("sub"), "Slack", vec![png()], at());
        assert!(saved.paths.is_empty());
        assert!(saved.cant_save);
    }
}
