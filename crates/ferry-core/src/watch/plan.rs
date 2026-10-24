//! What to watch: the settings turned into directories with roles, and the
//! status each one reports to the Settings pane.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::capture::{CaptureDir, CaptureRule};
use super::filter::is_inside;
use super::probe::PROBE_TIMEOUT;
use crate::fsutil::with_timeout;
use crate::i18n::t;
use crate::selfwrite::key;
use crate::settings::{Watch, contract_home, expand_home};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DirState {
    Watching,
    Missing,
    Paused,
    Denied,
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DirStatus {
    /// A folder: the stored path string. A capture folder: `~`-contracted.
    pub path: String,
    pub state: DirState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct WatchStatus {
    pub captures: Vec<DirStatus>,
    pub folders: Vec<DirStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolve {
    Dir(PathBuf),
    Missing,
    Denied,
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum Role {
    /// `slot` is the capture folder's place in `WatchStatus.captures`.
    Capture {
        rule: CaptureRule,
        slot: usize,
    },
    Folder {
        index: usize,
    },
}

#[derive(Debug, Clone)]
pub struct PlannedDir {
    pub dir: PathBuf,
    pub key: PathBuf,
    pub role: Role,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub dirs: Vec<PlannedDir>,
    pub status: WatchStatus,
    pub watch: Watch,
    /// Keyed (see `selfwrite::key`).
    pub excluded: Vec<PathBuf>,
}

/// Captures first, then folders in settings order. Capture watching off (both
/// toggles) means the capture folders are never listed, so macOS never asks for
/// Desktop access.
pub fn build(
    watch: &Watch,
    captures: &[CaptureDir],
    home: &Path,
    excluded: &[PathBuf],
    resolve: &mut dyn FnMut(&Path) -> Resolve,
) -> Plan {
    let mut plan = Plan {
        dirs: Vec::new(),
        status: WatchStatus::default(),
        watch: watch.clone(),
        excluded: excluded.iter().map(|e| key(e)).collect(),
    };
    let capturing = watch.captures.screenshots || watch.captures.recordings;
    for (slot, capture) in captures.iter().enumerate() {
        let state = if capturing {
            plan.resolve(
                &capture.dir,
                Role::Capture {
                    rule: capture.rule.clone(),
                    slot,
                },
                resolve,
            )
        } else {
            DirState::Paused
        };
        plan.status.captures.push(DirStatus {
            path: contract_home(&capture.dir, home),
            state,
        });
    }
    for (index, folder) in watch.folders.iter().enumerate() {
        let dir = expand_home(&folder.path, home);
        let state = if folder.paused {
            DirState::Paused
        } else if excluded.iter().any(|e| is_inside(&dir, e)) {
            DirState::Failed {
                reason: t("watch.error.own_folder"),
            }
        } else {
            plan.resolve(&dir, Role::Folder { index }, resolve)
        };
        plan.status.folders.push(DirStatus {
            path: folder.path.clone(),
            state,
        });
    }
    plan
}

impl Plan {
    fn resolve(
        &mut self,
        dir: &Path,
        role: Role,
        resolve: &mut dyn FnMut(&Path) -> Resolve,
    ) -> DirState {
        match resolve(dir) {
            Resolve::Dir(dir) => {
                self.dirs.push(PlannedDir {
                    key: key(&dir),
                    dir,
                    role,
                });
                DirState::Watching
            }
            Resolve::Missing => DirState::Missing,
            Resolve::Denied => DirState::Denied,
            Resolve::Failed(reason) => DirState::Failed { reason },
        }
    }

    /// The planned directories `file` sits directly in, captures first. Empty
    /// for anything in one of Ferry's own folders.
    pub fn roles_for(&self, file: &Path) -> Vec<&PlannedDir> {
        if self.excluded.iter().any(|e| is_inside(file, e)) {
            return Vec::new();
        }
        let file = key(file);
        let Some(parent) = file.parent() else {
            return Vec::new();
        };
        self.dirs.iter().filter(|d| d.key == parent).collect()
    }

    /// Each directory once, even when two folders resolve to the same one.
    pub fn watch_dirs(&self) -> Vec<PathBuf> {
        let mut seen: Vec<&PathBuf> = Vec::new();
        let mut out = Vec::new();
        for d in &self.dirs {
            if !seen.contains(&&d.key) {
                seen.push(&d.key);
                out.push(d.dir.clone());
            }
        }
        out
    }

    /// The file's directories refused it: say so. Returns whether anything changed.
    pub fn mark_denied(&mut self, file: &Path) -> bool {
        let roles: Vec<Role> = self
            .roles_for(file)
            .iter()
            .map(|d| d.role.clone())
            .collect();
        let mut changed = false;
        for role in roles {
            let slot = match role {
                Role::Capture { slot, .. } => self.status.captures.get_mut(slot),
                Role::Folder { index } => self.status.folders.get_mut(index),
            };
            if let Some(status) = slot.filter(|s| s.state != DirState::Denied) {
                status.state = DirState::Denied;
                changed = true;
            }
        }
        changed
    }
}

/// Resolve `dir` on the real file system: it must be a readable folder.
pub fn resolve_dir(dir: &Path) -> Resolve {
    let dir = dir.to_owned();
    let listed = with_timeout(PROBE_TIMEOUT, move || {
        #[cfg(windows)]
        let normal = std::path::absolute(&dir)?;
        #[cfg(not(windows))]
        let normal = fs::canonicalize(&dir)?;
        fs::read_dir(&normal)?;
        Ok(normal)
    });
    match listed {
        Ok(dir) => Resolve::Dir(dir),
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => Resolve::Missing,
            io::ErrorKind::PermissionDenied => Resolve::Denied,
            io::ErrorKind::TimedOut => Resolve::Failed(t("watch.status.timed_out")),
            _ => Resolve::Failed(e.to_string()),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderProblem {
    NotAFolder,
    OwnFolder,
}

/// Can the user watch this folder? Ferry's own folders cannot be: a watcher on
/// the backups would optimise the backups.
pub fn check_folder(dir: &Path, excluded: &[PathBuf]) -> Result<(), FolderProblem> {
    if excluded.iter().any(|e| is_inside(dir, e)) {
        return Err(FolderProblem::OwnFolder);
    }
    if !dir.is_dir() {
        return Err(FolderProblem::NotAFolder);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::WatchedFolder;
    use std::collections::HashMap;

    fn folder(path: &str) -> WatchedFolder {
        WatchedFolder {
            path: path.into(),
            ..WatchedFolder::default()
        }
    }

    const HOME: &str = "/home/me";

    fn build_with(
        watch: &Watch,
        captures: &[CaptureDir],
        excluded: &[PathBuf],
        known: &[(&str, Resolve)],
    ) -> Plan {
        let known: HashMap<PathBuf, Resolve> = known
            .iter()
            .map(|(p, r)| (PathBuf::from(p), r.clone()))
            .collect();
        build(watch, captures, Path::new(HOME), excluded, &mut |d| {
            known.get(d).cloned().unwrap_or(Resolve::Missing)
        })
    }

    fn dir(p: &str) -> Resolve {
        Resolve::Dir(PathBuf::from(p))
    }

    #[test]
    fn statuses_follow_each_folder_in_settings_order() {
        let watch = Watch {
            folders: vec![folder("~/A"), folder("~/B"), folder("~/C"), folder("~/D")],
            ..Watch::default()
        };
        let mut watch = watch;
        watch.folders[2].paused = true;
        let plan = build_with(
            &watch,
            &[],
            &[],
            &[
                ("/home/me/A", dir("/home/me/A")),
                ("/home/me/D", Resolve::Denied),
            ],
        );
        let states: Vec<_> = plan
            .status
            .folders
            .iter()
            .map(|s| s.state.clone())
            .collect();
        assert_eq!(
            states,
            [
                DirState::Watching,
                DirState::Missing,
                DirState::Paused,
                DirState::Denied
            ]
        );
        assert_eq!(plan.status.folders[0].path, "~/A");
    }

    #[test]
    fn both_capture_toggles_off_pauses_the_capture_folders() {
        let caps = [CaptureDir {
            dir: "/home/me/Desktop".into(),
            rule: CaptureRule::Everything,
        }];
        let known = [("/home/me/Desktop", dir("/home/me/Desktop"))];
        let off = build_with(&Watch::default(), &caps, &[], &known);
        assert_eq!(off.status.captures[0].state, DirState::Paused);
        assert_eq!(off.status.captures[0].path, "~/Desktop");
        assert!(off.dirs.is_empty(), "never listed, so no Desktop prompt");
        let mut on = Watch::default();
        on.captures.screenshots = true;
        let on = build_with(&on, &caps, &[], &known);
        assert_eq!(on.status.captures[0].state, DirState::Watching);
        assert_eq!(on.dirs.len(), 1);
    }

    #[test]
    fn ferrys_own_folders_are_refused() {
        let excluded = [PathBuf::from("/app/backups")];
        let watch = Watch {
            folders: vec![folder("/app/backups/x"), folder("/app/backups")],
            ..Watch::default()
        };
        let plan = build_with(&watch, &[], &excluded, &[]);
        for status in &plan.status.folders {
            assert_eq!(
                status.state,
                DirState::Failed {
                    reason: t("watch.error.own_folder")
                }
            );
        }
        assert!(plan.dirs.is_empty());
        assert_eq!(
            check_folder(Path::new("/app/backups/x"), &excluded),
            Err(FolderProblem::OwnFolder)
        );
    }

    #[test]
    fn a_real_folder_is_checked_and_a_file_is_not_a_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("f");
        fs::write(&file, b"x").unwrap();
        assert_eq!(check_folder(tmp.path(), &[]), Ok(()));
        assert_eq!(check_folder(&file, &[]), Err(FolderProblem::NotAFolder));
        assert_eq!(resolve_dir(&tmp.path().join("nope")), Resolve::Missing);
        assert_eq!(resolve_dir(&file), Resolve::Missing);
        assert!(matches!(resolve_dir(tmp.path()), Resolve::Dir(_)));
    }

    #[test]
    fn roles_put_captures_first_and_skip_subfolders_and_excluded_roots() {
        let caps = [CaptureDir {
            dir: "/home/me/Desktop".into(),
            rule: CaptureRule::Everything,
        }];
        let mut watch = Watch {
            folders: vec![folder("~/Desktop"), folder("~/Other")],
            ..Watch::default()
        };
        watch.captures.screenshots = true;
        let excluded = [PathBuf::from("/home/me/Desktop/.work")];
        let plan = build_with(
            &watch,
            &caps,
            &excluded,
            &[
                ("/home/me/Desktop", dir("/home/me/Desktop")),
                ("/home/me/Other", dir("/home/me/Other")),
            ],
        );
        let roles = plan.roles_for(Path::new("/home/me/Desktop/a.png"));
        assert_eq!(roles.len(), 2);
        assert!(matches!(roles[0].role, Role::Capture { .. }));
        assert!(matches!(roles[1].role, Role::Folder { index: 0 }));
        assert!(
            plan.roles_for(Path::new("/home/me/Desktop/sub/a.png"))
                .is_empty()
        );
        assert!(
            plan.roles_for(Path::new("/home/me/Desktop/.work/a.png"))
                .is_empty()
        );
        assert_eq!(plan.watch_dirs().len(), 2);
    }

    #[test]
    fn a_denied_folder_is_marked_once() {
        let watch = Watch {
            folders: vec![folder("~/A")],
            ..Watch::default()
        };
        let mut plan = build_with(&watch, &[], &[], &[("/home/me/A", dir("/home/me/A"))]);
        assert!(plan.mark_denied(Path::new("/home/me/A/x.png")));
        assert!(!plan.mark_denied(Path::new("/home/me/A/x.png")));
        assert_eq!(plan.status.folders[0].state, DirState::Denied);
    }
}
