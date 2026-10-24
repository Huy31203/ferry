//! `notify` events boiled down to what the watcher cares about.

use std::path::PathBuf;

use notify::EventKind;
use notify::event::ModifyKind;
use notify::event::RenameMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsEvent {
    /// Created, or renamed into the folder: starts a wait.
    Arrived(PathBuf),
    /// Its data changed: extends a wait that is already running.
    Changed(PathBuf),
    Gone(PathBuf),
}

pub fn from_notify(ev: &notify::Event) -> Vec<FsEvent> {
    let arrived = || ev.paths.iter().cloned().map(FsEvent::Arrived).collect();
    let changed = || ev.paths.iter().cloned().map(FsEvent::Changed).collect();
    match &ev.kind {
        EventKind::Create(_) => arrived(),
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => ev
            .paths
            .first()
            .cloned()
            .map(FsEvent::Arrived)
            .into_iter()
            .collect(),
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => ev
            .paths
            .first()
            .cloned()
            .map(FsEvent::Gone)
            .into_iter()
            .collect(),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
            let mut out = Vec::new();
            out.extend(ev.paths.first().cloned().map(FsEvent::Gone));
            out.extend(ev.paths.get(1).cloned().map(FsEvent::Arrived));
            out
        }
        // The old path of a macOS rename no longer exists and is dropped by the probe.
        EventKind::Modify(ModifyKind::Name(_)) => arrived(),
        EventKind::Modify(ModifyKind::Metadata(_)) | EventKind::Access(_) => Vec::new(),
        EventKind::Modify(_) => changed(),
        EventKind::Remove(_) => ev.paths.iter().cloned().map(FsEvent::Gone).collect(),
        EventKind::Any | EventKind::Other => changed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::Event;
    use notify::event::{
        AccessKind, CreateKind, DataChange, MetadataKind, ModifyKind, RemoveKind, RenameMode,
    };
    use std::path::Path;

    fn ev(kind: EventKind, paths: &[&str]) -> Vec<FsEvent> {
        let mut e = Event::new(kind);
        for p in paths {
            e = e.add_path(Path::new(p).to_owned());
        }
        from_notify(&e)
    }
    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn arrivals_are_creates_and_renames_into_a_folder() {
        let name = |m| EventKind::Modify(ModifyKind::Name(m));
        assert_eq!(
            ev(EventKind::Create(CreateKind::File), &["/a"]),
            [FsEvent::Arrived(p("/a"))]
        );
        assert_eq!(
            ev(name(RenameMode::To), &["/a"]),
            [FsEvent::Arrived(p("/a"))]
        );
        assert_eq!(
            ev(name(RenameMode::From), &["/a"]),
            [FsEvent::Gone(p("/a"))]
        );
        assert_eq!(
            ev(name(RenameMode::Both), &["/a", "/b"]),
            [FsEvent::Gone(p("/a")), FsEvent::Arrived(p("/b"))]
        );
        assert_eq!(
            ev(name(RenameMode::Any), &["/a"]),
            [FsEvent::Arrived(p("/a"))]
        );
        assert_eq!(
            ev(
                EventKind::Modify(ModifyKind::Data(DataChange::Content)),
                &["/a"]
            ),
            [FsEvent::Changed(p("/a"))]
        );
        assert!(
            ev(
                EventKind::Modify(ModifyKind::Metadata(MetadataKind::Extended)),
                &["/a"]
            )
            .is_empty()
        );
        assert!(ev(EventKind::Access(AccessKind::Any), &["/a"]).is_empty());
        assert_eq!(
            ev(EventKind::Remove(RemoveKind::File), &["/a"]),
            [FsEvent::Gone(p("/a"))]
        );
    }
}
