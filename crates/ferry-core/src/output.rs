//! Where a file job's result goes (spec §4.4): over the source, beside it
//! under a name template, in a chosen folder, or in the working folder.
//! Clipboard jobs always use their working file and never come here.

use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use crate::settings::{self, Location, Output, OutputRule};
use crate::template;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    /// Same format as the source (also downscale and crop).
    Optimised,
    Converted,
}

pub struct Target<'a> {
    /// The file job's own file.
    pub source: &'a Path,
    pub job: &'a str,
    /// The result's extension.
    pub ext: &'a str,
    /// The result's size, for `%z`.
    pub dims: (u32, u32),
    pub kind: OutputKind,
}

/// The path a result should have, before `unique`. A path equal to the
/// source means "overwrite the source".
pub fn resolve(
    t: &Target,
    rules: &Output,
    work_dir: &Path,
    home: &Path,
    today: NaiveDate,
) -> PathBuf {
    let rule = match t.kind {
        OutputKind::Optimised => &rules.optimised,
        OutputKind::Converted => &rules.converted,
    };
    let stem = t
        .source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folder = t.source.parent().unwrap_or(Path::new(""));
    let named = |dir: &Path| {
        let parent = folder
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ctx = template::Ctx {
            stem: &stem,
            ext: t.ext,
            width: t.dims.0,
            height: t.dims.1,
            parent: &parent,
            date: today,
        };
        dir.join(template::safe_file_name(&template::render(
            &rule.template,
            &ctx,
        )))
    };
    match rule.location {
        Location::InPlace => match t.kind {
            OutputKind::Optimised => t.source.to_owned(),
            OutputKind::Converted => folder.join(format!("{stem}.{}", t.ext)),
        },
        Location::SameFolder => named(folder),
        Location::Folder => named(&settings::expand_home(&rule.folder, home)),
        Location::Temporary => work_dir
            .join("files")
            .join(t.job)
            .join(format!("{stem}.{}", t.ext)),
    }
}

/// The File handling pane's live example: where `~/Desktop/shot.png`
/// (1920×1080) goes under `rule`, as a WebP when `kind` is Converted, with
/// `~` for the home folder. `Err` lists the template's unknown tokens, when
/// the location uses the template.
pub fn example(
    kind: OutputKind,
    rule: &OutputRule,
    home: &Path,
    work_dir: &Path,
    today: NaiveDate,
) -> Result<String, Vec<String>> {
    if matches!(rule.location, Location::SameFolder | Location::Folder) {
        template::validate(&rule.template)?;
    }
    let rules = Output {
        optimised: rule.clone(),
        converted: rule.clone(),
        ..Output::default()
    };
    let source = home.join("Desktop").join("shot.png");
    let target = Target {
        source: &source,
        job: "example",
        ext: match kind {
            OutputKind::Optimised => "png",
            OutputKind::Converted => "webp",
        },
        dims: (1920, 1080),
        kind,
    };
    let path = resolve(&target, &rules, work_dir, home, today);
    Ok(settings::contract_home(&path, home))
}

/// `path`, or `name (2).ext`, `name (3).ext`, … when another file already
/// has it. `own` (this job's previous result) never counts as taken.
pub fn unique(path: &Path, own: Option<&Path>) -> PathBuf {
    unique_except(path, own, None)
}

/// Like `unique`, and `reserved` counts as taken even when it is free: a
/// twin's result must never land on its parent's source path.
pub fn unique_except(path: &Path, own: Option<&Path>, reserved: Option<&Path>) -> PathBuf {
    let free = |p: &Path| (!p.exists() || own == Some(p)) && reserved != Some(p);
    if free(path) {
        return path.to_owned();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (2..)
        .map(|n| path.with_file_name(format!("{stem} ({n}){ext}")))
        .find(|candidate| free(candidate))
        .expect("an unused name exists")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Location, Output};
    use chrono::NaiveDate;
    use std::fs;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()
    }

    fn at(kind: OutputKind, ext: &str, rules: &Output) -> PathBuf {
        let source = Path::new("/u/Desktop/shot.png");
        let target = Target {
            source,
            job: "j1",
            ext,
            dims: (300, 200),
            kind,
        };
        resolve(&target, rules, Path::new("/w"), Path::new("/h"), today())
    }

    fn rules(location: Location, template: &str) -> Output {
        let mut rules = Output::default();
        for rule in [&mut rules.optimised, &mut rules.converted] {
            rule.location = location;
            rule.template = template.into();
        }
        rules
    }

    #[test]
    fn in_place_overwrites_or_swaps_the_extension() {
        let defaults = Output::default();
        assert_eq!(
            at(OutputKind::Optimised, "png", &defaults),
            Path::new("/u/Desktop/shot.png")
        );
        assert_eq!(
            at(OutputKind::Converted, "webp", &defaults),
            Path::new("/u/Desktop/shot.webp")
        );
    }

    #[test]
    fn same_folder_renders_the_template_beside_the_source() {
        let defaults = Output {
            optimised: rules(Location::SameFolder, "%f_optimised.%e").optimised,
            ..Output::default()
        };
        assert_eq!(
            at(OutputKind::Optimised, "png", &defaults),
            Path::new("/u/Desktop/shot_optimised.png")
        );
        let tokens = rules(Location::SameFolder, "%P-%z-%y%m%d.%e");
        assert_eq!(
            at(OutputKind::Converted, "webp", &tokens),
            Path::new("/u/Desktop/Desktop-300x200-20261022.webp")
        );
    }

    #[test]
    fn a_specific_folder_expands_home() {
        let mut folder = rules(Location::Folder, "%f.%e");
        folder.converted.folder = "~/Pictures/Ferry".into();
        assert_eq!(
            at(OutputKind::Converted, "webp", &folder),
            Path::new("/h").join("Pictures/Ferry").join("shot.webp")
        );
    }

    #[test]
    fn temporary_goes_to_the_jobs_own_working_folder() {
        assert_eq!(
            at(
                OutputKind::Optimised,
                "png",
                &rules(Location::Temporary, "%f.%e")
            ),
            Path::new("/w").join("files").join("j1").join("shot.png")
        );
    }

    #[test]
    fn a_template_cannot_escape_its_folder() {
        assert_eq!(
            at(
                OutputKind::Converted,
                "png",
                &rules(Location::SameFolder, "../%f.%e")
            ),
            Path::new("/u/Desktop/.._shot.png")
        );
    }

    #[test]
    fn a_taken_name_gets_a_number_unless_it_is_our_own() {
        let dir = tempfile::tempdir().unwrap();
        let wanted = dir.path().join("shot.webp");
        assert_eq!(unique(&wanted, None), wanted);
        fs::write(&wanted, b"x").unwrap();
        assert_eq!(unique(&wanted, None), dir.path().join("shot (2).webp"));
        assert_eq!(unique(&wanted, Some(&wanted)), wanted);
        fs::write(dir.path().join("shot (2).webp"), b"x").unwrap();
        assert_eq!(unique(&wanted, None), dir.path().join("shot (3).webp"));
    }

    #[test]
    fn a_reserved_name_counts_as_taken_even_when_free() {
        let dir = tempfile::tempdir().unwrap();
        let wanted = dir.path().join("shot.png");
        assert_eq!(
            unique_except(&wanted, None, Some(&wanted)),
            dir.path().join("shot (2).png")
        );
        assert_eq!(unique_except(&wanted, None, None), wanted);
    }

    fn rule(location: Location, template: &str) -> OutputRule {
        OutputRule {
            location,
            template: template.into(),
            ..OutputRule::default()
        }
    }

    fn example_of(kind: OutputKind, rule: &OutputRule) -> Result<String, Vec<String>> {
        example(kind, rule, Path::new("/h"), Path::new("/w"), today())
    }

    #[test]
    fn the_example_shows_where_a_desktop_screenshot_goes() {
        let in_place = rule(Location::InPlace, "%f.%e");
        assert_eq!(
            example_of(OutputKind::Optimised, &in_place).unwrap(),
            "~/Desktop/shot.png"
        );
        assert_eq!(
            example_of(OutputKind::Converted, &in_place).unwrap(),
            "~/Desktop/shot.webp"
        );
        assert_eq!(
            example_of(
                OutputKind::Optimised,
                &rule(Location::SameFolder, "%f_optimised.%e")
            )
            .unwrap(),
            "~/Desktop/shot_optimised.png"
        );
        let folder = OutputRule {
            folder: "~/Pictures/Ferry".into(),
            ..rule(Location::Folder, "%f_%z.%e")
        };
        assert_eq!(
            example_of(OutputKind::Converted, &folder).unwrap(),
            "~/Pictures/Ferry/shot_1920x1080.webp"
        );
    }

    #[test]
    fn the_example_names_unknown_tokens_only_where_the_template_is_used() {
        assert_eq!(
            example_of(
                OutputKind::Optimised,
                &rule(Location::SameFolder, "%f%q.%e")
            ),
            Err(vec!["%q".to_string()])
        );
        assert!(example_of(OutputKind::Optimised, &rule(Location::InPlace, "%q")).is_ok());
    }
}
