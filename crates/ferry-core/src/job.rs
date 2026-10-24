//! The shape of a job as it travels from the pipeline to the result cards.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::edit::Edits;
use crate::sniff::Format;

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
    /// The original's size as shown (JPEG orientation applied).
    pub dims_before: Option<(u32, u32)>,
    /// The result's size when an edit changed it.
    pub dims_after: Option<(u32, u32)>,
    /// The current result's format; `None` on a notice.
    pub format: Option<Format>,
    pub converted: bool,
    /// The edit tools apply: the original is still backed up, the job is idle,
    /// and it is not an animated WebP or APNG.
    pub can_edit: bool,
    /// The recipe the sliders and popovers start from.
    pub edits: Edits,
    /// An amber note, such as "Couldn't reach 1 MB — try downscaling".
    pub note: Option<String>,
    /// The original's format: the format bar's GIF chip and "First frame only".
    pub source_format: Option<Format>,
    /// Why the edit tools are off, for their tooltip; `None` when they are on
    /// or the job is busy.
    pub edit_hint: Option<String>,
    // stream A: video
    #[serde(default)]
    pub video: Option<VideoMeta>,
    // stream B: audio
    #[serde(default)]
    pub audio: Option<crate::audio::AudioCard>,
}

// stream A: video
/// What a video card shows besides what every card shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoMeta {
    /// The current result's; the original's while the original is the result.
    pub duration_ms: u64,
    /// The current result's codec label, such as "HEVC".
    pub codec: String,
    pub fps: Option<f32>,
    /// The current result has audio.
    pub audio: bool,
    /// The backup: compare's left side.
    pub original: Option<PathBuf>,
    pub original_duration_ms: u64,
    /// The job's hardware has an HEVC encoder: the format bar and menu offer HEVC.
    pub hevc_encoder: bool,
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
            dims_before: Some((2880, 1800)),
            dims_after: Some((1440, 900)),
            format: Some(Format::WebP),
            converted: true,
            can_edit: true,
            edits: Edits::default(),
            note: Some("Couldn't reach 1 MB".into()),
            source_format: Some(Format::Png),
            edit_hint: None,
            video: None,
            audio: None,
        };
        let json = serde_json::to_string(&update).unwrap();
        assert!(json.contains(r#""source":"clipboard""#), "{json}");
        assert_eq!(serde_json::from_str::<JobUpdate>(&json).unwrap(), update);
        // stream B: audio
        let audio = JobUpdate {
            audio: Some(crate::audio::AudioCard {
                bitrate_before: Some(320),
                bitrate_after: Some(192),
                lossless: false,
                has_cover: true,
            }),
            ..update
        };
        let json = serde_json::to_string(&audio).unwrap();
        assert!(json.contains(r#""has_cover":true"#), "{json}");
        assert_eq!(serde_json::from_str::<JobUpdate>(&json).unwrap(), audio);
    }

    // stream A: video
    #[test]
    fn a_video_update_round_trips_through_json() {
        let meta = VideoMeta {
            duration_ms: 5000,
            codec: "HEVC".into(),
            fps: Some(29.97),
            audio: true,
            original: Some(PathBuf::from("/b/a.mp4")),
            original_duration_ms: 10_000,
            hevc_encoder: true,
        };
        let update = JobUpdate {
            id: JobId::generate(),
            state: JobState::Done,
            name: "clip".into(),
            ext: "mp4".into(),
            before: 2_000_000,
            after: Some(310_000),
            detail: String::new(),
            thumb_path: None,
            source: Source::File,
            can_restore: true,
            can_retry: false,
            output: None,
            dims_before: Some((1920, 1080)),
            dims_after: None,
            format: Some(Format::Hevc),
            converted: false,
            can_edit: true,
            edits: Edits::default(),
            note: None,
            source_format: Some(Format::Hevc),
            edit_hint: None,
            video: Some(meta),
            audio: None,
        };
        let json = serde_json::to_string(&update).unwrap();
        assert_eq!(serde_json::from_str::<JobUpdate>(&json).unwrap(), update);
        // A card from before this field existed has no video.
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value.as_object_mut().unwrap().remove("video");
        let old: JobUpdate = serde_json::from_value(value).unwrap();
        assert_eq!(old.video, None);
    }
}
