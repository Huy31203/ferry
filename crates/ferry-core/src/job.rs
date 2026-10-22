//! The shape of a job as it travels from the pipeline to the result cards.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Unique across app launches, because it names the job's backup folder.
/// Only `0-9`, `a-f` and `-`, so it is safe as a folder name on every OS.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(String);

impl JobId {
    pub fn generate() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        Self(format!(
            "{millis:x}-{:x}",
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Optimising { progress: f32, step: String },
    Done,
    AlreadyOptimal,
    Larger,
    Error { reason: String },
    Restored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Clipboard,
    File,
}

/// Everything a result card shows, sent each time a job changes. Sizes are
/// bytes; `after` is `None` until the output exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobUpdate {
    pub id: JobId,
    pub state: JobState,
    pub name: String,
    pub ext: String,
    pub before: u64,
    pub after: Option<u64>,
    pub detail: String,
    pub thumb_path: Option<PathBuf>,
    pub source: Source,
    pub can_restore: bool,
    /// An Error card for a real job; notices such as "Nothing to optimise" have no retry.
    pub can_retry: bool,
    /// The file the card's copy, drag and reveal act on, once it exists.
    pub output: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn job_ids_are_unique_and_safe_as_folder_names() {
        let ids: HashSet<String> = (0..1000)
            .map(|_| JobId::generate().as_str().to_owned())
            .collect();
        assert_eq!(ids.len(), 1000);
        assert!(
            ids.iter()
                .all(|id| id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        );
    }

    #[test]
    fn states_serialise_with_a_kind_tag() {
        let json = |s: &JobState| serde_json::to_string(s).unwrap();
        assert_eq!(json(&JobState::Queued), r#"{"kind":"queued"}"#);
        assert_eq!(
            json(&JobState::AlreadyOptimal),
            r#"{"kind":"already_optimal"}"#
        );
        assert_eq!(
            json(&JobState::Optimising {
                progress: 0.5,
                step: "Encoding JPEG".into()
            }),
            r#"{"kind":"optimising","progress":0.5,"step":"Encoding JPEG"}"#
        );
        assert_eq!(
            json(&JobState::Error {
                reason: "Read-only folder".into()
            }),
            r#"{"kind":"error","reason":"Read-only folder"}"#
        );
    }

    #[test]
    fn an_update_round_trips_through_json() {
        let update = JobUpdate {
            id: JobId::generate(),
            state: JobState::Done,
            name: "shot".into(),
            ext: "png".into(),
            before: 2_000_000,
            after: Some(310_000),
            detail: "Quantised to 256 colours".into(),
            thumb_path: Some(PathBuf::from("/tmp/thumb.png")),
            source: Source::Clipboard,
            can_restore: true,
            can_retry: false,
            output: Some(PathBuf::from("/tmp/work/shot.png")),
        };
        let json = serde_json::to_string(&update).unwrap();
        assert!(json.contains(r#""source":"clipboard""#), "{json}");
        assert_eq!(serde_json::from_str::<JobUpdate>(&json).unwrap(), update);
    }
}
