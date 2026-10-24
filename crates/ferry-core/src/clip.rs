//! What goes on the clipboard for a result (stream I, R§4 "Clipboard output"):
//! an image, files, or files with their paths as text. Also the "accumulate
//! results" collection, keyed by job so it follows renames and conversions.

use std::path::{Path, PathBuf};

use crate::job::{JobId, JobState, Source};

/// A clipboard job's image result, or a file job's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Produced {
    Image,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipWrite {
    Image,
    Files(Vec<PathBuf>),
    Paths(Vec<PathBuf>),
}

impl ClipWrite {
    /// The same write with `old` replaced by `new`; `None` when `old` is not in it.
    pub fn renamed(&self, old: &Path, new: &Path) -> Option<ClipWrite> {
        let swap = |paths: &[PathBuf]| {
            paths.contains(&old.to_path_buf()).then(|| {
                paths
                    .iter()
                    .map(|p| {
                        if p == old {
                            new.to_path_buf()
                        } else {
                            p.clone()
                        }
                    })
                    .collect()
            })
        };
        match self {
            ClipWrite::Image => None,
            ClipWrite::Files(paths) => swap(paths).map(ClipWrite::Files),
            ClipWrite::Paths(paths) => swap(paths).map(ClipWrite::Paths),
        }
    }
}

/// "Accumulate results", oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Collected(Vec<(JobId, PathBuf)>);

impl Collected {
    /// Replace this job's path in place, else append.
    pub fn put(&mut self, id: &JobId, path: &Path) {
        match self.0.iter_mut().find(|(j, _)| j == id) {
            Some(entry) => entry.1 = path.to_path_buf(),
            None => self.0.push((id.clone(), path.to_path_buf())),
        }
    }

    /// Replace only if the job is in it.
    pub fn follow(&mut self, id: &JobId, path: &Path) {
        if let Some(entry) = self.0.iter_mut().find(|(j, _)| j == id) {
            entry.1 = path.to_path_buf();
        }
    }

    pub fn retain(&mut self, keep: impl Fn(&JobId) -> bool) {
        self.0.retain(|(id, _)| keep(id));
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// In order, only files that exist.
    pub fn paths(&self) -> Vec<PathBuf> {
        self.0
            .iter()
            .map(|(_, p)| p)
            .filter(|p| p.exists())
            .cloned()
            .collect()
    }
}

/// What to write for a result at `file`. `collected` is `Some` when accumulating
/// (the caller has already `put` this job into it).
pub fn plan(
    copy_paths: bool,
    produced: Produced,
    file: &Path,
    collected: Option<&Collected>,
) -> ClipWrite {
    if let Some(collected) = collected {
        let mut paths = collected.paths();
        if paths.is_empty() {
            paths.push(file.to_path_buf());
        }
        return if copy_paths {
            ClipWrite::Paths(paths)
        } else {
            ClipWrite::Files(paths)
        };
    }
    if copy_paths {
        return ClipWrite::Paths(vec![file.to_path_buf()]);
    }
    match produced {
        Produced::Image => ClipWrite::Image,
        Produced::File => ClipWrite::Files(vec![file.to_path_buf()]),
    }
}

/// Whether the app must still put a finished run's result on the clipboard
/// (the pipeline did not).
pub fn copy_after_run(
    source: Source,
    state: &JobState,
    first_run: bool,
    accumulate: bool,
    auto_copy: bool,
) -> bool {
    match source {
        Source::Clipboard => {
            first_run && accumulate && matches!(state, JobState::AlreadyOptimal | JobState::Larger)
        }
        Source::File => {
            auto_copy
                && matches!(
                    state,
                    JobState::Done | JobState::AlreadyOptimal | JobState::Larger
                )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, b"x").unwrap();
        p
    }

    #[test]
    fn put_replaces_a_jobs_path_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (JobId::generate(), JobId::generate());
        let (a_png, b_png, a_webp) = (
            touch(dir.path(), "a.png"),
            touch(dir.path(), "b.png"),
            touch(dir.path(), "a.webp"),
        );
        let mut c = Collected::default();
        c.put(&a, &a_png);
        c.put(&b, &b_png);
        c.put(&a, &a_webp);
        assert_eq!(c.paths(), [a_webp, b_png]);
    }

    #[test]
    fn follow_never_adds_a_job() {
        let dir = tempfile::tempdir().unwrap();
        let a = JobId::generate();
        let hero = touch(dir.path(), "hero.png");
        let mut c = Collected::default();
        c.follow(&a, &hero);
        assert!(c.paths().is_empty());
        c.put(&a, &touch(dir.path(), "a.png"));
        c.follow(&a, &hero);
        assert_eq!(c.paths(), [hero]);
    }

    #[test]
    fn paths_skip_files_that_are_gone() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (touch(dir.path(), "a.png"), touch(dir.path(), "b.png"));
        let mut c = Collected::default();
        c.put(&JobId::generate(), &a);
        c.put(&JobId::generate(), &b);
        std::fs::remove_file(&a).unwrap();
        assert_eq!(c.paths(), [b]);
    }

    #[test]
    fn plan_without_accumulate() {
        let f = PathBuf::from("/w/f.png");
        assert_eq!(plan(false, Produced::Image, &f, None), ClipWrite::Image);
        assert_eq!(
            plan(false, Produced::File, &f, None),
            ClipWrite::Files(vec![f.clone()])
        );
        assert_eq!(
            plan(true, Produced::Image, &f, None),
            ClipWrite::Paths(vec![f])
        );
    }

    #[test]
    fn an_accumulated_plan_always_holds_the_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b, f) = (
            touch(dir.path(), "a.png"),
            touch(dir.path(), "b.png"),
            touch(dir.path(), "f.png"),
        );
        let mut c = Collected::default();
        c.put(&JobId::generate(), &a);
        c.put(&JobId::generate(), &b);
        std::fs::remove_file(&a).unwrap();
        std::fs::remove_file(&b).unwrap();
        assert_eq!(
            plan(false, Produced::Image, &f, Some(&c)),
            ClipWrite::Files(vec![f.clone()])
        );
        let (a, b) = (touch(dir.path(), "a.png"), touch(dir.path(), "b.png"));
        assert_eq!(
            plan(true, Produced::Image, &f, Some(&c)),
            ClipWrite::Paths(vec![a, b])
        );
    }

    #[test]
    fn a_rename_follows_into_the_written_files() {
        let (a, b, h, x) = (
            PathBuf::from("/w/a.png"),
            PathBuf::from("/w/b.png"),
            PathBuf::from("/w/hero.png"),
            PathBuf::from("/w/x.png"),
        );
        assert_eq!(
            ClipWrite::Files(vec![a.clone(), b.clone()]).renamed(&a, &h),
            Some(ClipWrite::Files(vec![h.clone(), b]))
        );
        assert_eq!(ClipWrite::Paths(vec![a.clone()]).renamed(&x, &h), None);
        assert_eq!(ClipWrite::Image.renamed(&a, &h), None);
    }

    #[test]
    fn only_unwritten_results_are_copied_after_a_run() {
        let err = JobState::Error { reason: "x".into() };
        let table = [
            (
                Source::Clipboard,
                JobState::AlreadyOptimal,
                true,
                true,
                false,
                true,
            ),
            (Source::Clipboard, JobState::Larger, true, true, false, true),
            (
                Source::Clipboard,
                JobState::Larger,
                false,
                true,
                false,
                false,
            ),
            (
                Source::Clipboard,
                JobState::Larger,
                true,
                false,
                false,
                false,
            ),
            (Source::Clipboard, JobState::Done, true, true, true, false),
            (Source::File, JobState::Done, true, false, true, true),
            (
                Source::File,
                JobState::AlreadyOptimal,
                true,
                false,
                true,
                true,
            ),
            (Source::File, JobState::Larger, true, false, true, true),
            (Source::File, JobState::Done, true, true, false, false),
            (Source::File, err, true, true, true, false),
            (Source::File, JobState::Restored, true, true, true, false),
        ];
        for (source, state, first, accumulate, auto, want) in table {
            assert_eq!(
                copy_after_run(source, &state, first, accumulate, auto),
                want,
                "{source:?} {state:?} first={first} accumulate={accumulate} auto={auto}"
            );
        }
    }
}
