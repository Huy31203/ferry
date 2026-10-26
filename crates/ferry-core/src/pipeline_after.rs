//! The copy step: runs on a finished result. It never overwrites; a taken name
//! gets ` (2)`, ` (3)`, …

use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::NaiveDate;

use crate::{
    fsutil,
    i18n::tf,
    output,
    settings::{contract_home, expand_home},
    template::{self, Ctx},
};

/// Where a copy goes: `template` rendered for the result (its stem, extension,
/// dimensions, its folder's name, today); the last path part is made safe with
/// `safe_file_name`; `~` expands with `home`; a relative folder is relative to
/// the result's folder.
pub fn copy_target(
    template: &str,
    result: &Path,
    dims: (u32, u32),
    home: &Path,
    today: NaiveDate,
) -> PathBuf {
    let text = |s: Option<&std::ffi::OsStr>| s.map(|s| s.to_string_lossy().into_owned());
    let stem = text(result.file_stem()).unwrap_or_default();
    let ext = text(result.extension()).unwrap_or_default();
    let beside = result.parent().unwrap_or(Path::new(""));
    let parent = text(beside.file_name()).unwrap_or_default();
    let rendered = template::render(
        template,
        &Ctx {
            stem: &stem,
            ext: &ext,
            width: dims.0,
            height: dims.1,
            parent: &parent,
            date: today,
        },
    );
    let (dir, file) = match rendered.rfind(['/', '\\']) {
        Some(0) => (&rendered[..1], &rendered[1..]),
        Some(i) => (&rendered[..i], &rendered[i + 1..]),
        None => ("", rendered.as_str()),
    };
    let folder = if dir.is_empty() {
        beside.to_owned()
    } else {
        let expanded = expand_home(dir, home);
        if expanded.is_absolute() {
            expanded
        } else {
            beside.join(expanded)
        }
    };
    folder.join(template::safe_file_name(file))
}

/// Copy `result` to `copy_target(...)`, or the next free name beside it,
/// creating folders. `Err` is the card text: the folder that can't be written.
pub fn copy_to(
    result: &Path,
    template: &str,
    dims: (u32, u32),
    home: &Path,
    today: NaiveDate,
) -> Result<PathBuf, String> {
    let wanted = copy_target(template, result, dims, home, today);
    let target = output::unique(&wanted, None);
    let folder = target.parent().unwrap_or(Path::new(""));
    // Through `atomic_replace_from`, which also tells the watchers the file is Ferry's own.
    fs::create_dir_all(folder)
        .and_then(|()| fsutil::atomic_replace_from(&target, result))
        .map_err(|_| tf("error.write", &[("dir", &contract_home(folder, home))]))?;
    // Best effort: the content is safe; a lost tag is not worth failing the step.
    let _ = fsutil::copy_xattrs(result, &target);
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()
    }

    #[test]
    fn the_target_renders_tokens_and_expands_home() {
        let home = Path::new("/home/me");
        assert_eq!(
            copy_target(
                "~/Pictures/twitter/%f_%z.%e",
                Path::new("/tmp/Desk/shot.png"),
                (1920, 1080),
                home,
                today()
            ),
            PathBuf::from("/home/me/Pictures/twitter/shot_1920x1080.png")
        );
        assert_eq!(
            copy_target(
                "out/%P-%y.%e",
                Path::new("/tmp/Desk/shot.png"),
                (1, 1),
                home,
                today()
            ),
            PathBuf::from("/tmp/Desk/out/Desk-2026.png")
        );
        assert_eq!(
            copy_target("~/x/a:b.%e", Path::new("/t/s.png"), (1, 1), home, today()),
            PathBuf::from("/home/me/x/a_b.png")
        );
        assert_eq!(
            copy_target("%f_small.%e", Path::new("/t/s.png"), (1, 1), home, today()),
            PathBuf::from("/t/s_small.png")
        );
    }

    #[test]
    fn a_copy_never_overwrites_and_lands_beside_with_a_number() {
        let dir = tempfile::tempdir().unwrap();
        let result = dir.path().join("shot.png");
        fs::write(&result, b"new").unwrap();
        let out = dir.path().join("out");
        fs::create_dir(&out).unwrap();
        fs::write(out.join("shot.png"), b"old").unwrap();
        let tpl = format!("{}/%f.%e", out.display());
        let copied = copy_to(&result, &tpl, (1, 1), dir.path(), today()).unwrap();
        assert_eq!(copied, out.join("shot (2).png"));
        assert_eq!(
            (
                fs::read(out.join("shot.png")).unwrap(),
                fs::read(&copied).unwrap()
            ),
            (b"old".to_vec(), b"new".to_vec())
        );
    }

    #[test]
    fn a_copy_is_known_to_the_watchers_as_ferrys_own() {
        let dir = tempfile::tempdir().unwrap();
        let result = dir.path().join("shot.png");
        fs::write(&result, b"x").unwrap();
        let tpl = format!("{}/own/%f.%e", dir.path().display());
        let copied = copy_to(&result, &tpl, (1, 1), dir.path(), today()).unwrap();
        assert!(crate::selfwrite::is_own(
            &copied,
            &crate::selfwrite::Stamp::of(&copied).unwrap()
        ));
    }

    #[cfg(unix)]
    #[test]
    fn an_unwritable_target_is_an_error_naming_the_folder() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let result = dir.path().join("shot.png");
        fs::write(&result, b"x").unwrap();
        let locked = dir.path().join("locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();
        let err = copy_to(
            &result,
            &format!("{}/%f.%e", locked.display()),
            (1, 1),
            dir.path(),
            today(),
        )
        .unwrap_err();
        assert!(err.contains("locked"), "{err}");
    }
}
