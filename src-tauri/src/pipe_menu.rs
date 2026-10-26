//! The pipeline items of the card's menu (stream F): re-optimise with an
//! encoder, strip metadata, run a library pipeline, show the pipeline that
//! made the result. `menu.rs` builds the menu and routes the items here.

use ferry_core::edit::EditsPatch;
use ferry_core::i18n::t;
use ferry_core::job::{JobId, JobState, JobUpdate};
use ferry_core::optimise::Encoder;
use ferry_core::pipeline_steps::Media;
use ferry_core::sniff::Format;
use tauri::menu::{CheckMenuItem, IsMenuItem, MenuItem, Submenu};
use tauri::{AppHandle, Manager, Wry};

use crate::menu::{Action, item_id};
use crate::pipe_jobs::{PipeState, run_on_card};
use crate::state::AppState;
use crate::{jobs, pipe_windows};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeAction {
    Encoder(Encoder),
    Strip,
    Run(u32),
    Producer,
}

const ENCODERS: [(Encoder, &str); 3] = [
    (Encoder::Fast, "fast"),
    (Encoder::Small, "small"),
    (Encoder::VisuallyLossless, "visually_lossless"),
];

impl PipeAction {
    /// The item id's part after `pipe-`: "encoder-fast", "strip", "run-3", "producer".
    pub fn key(self) -> String {
        match self {
            PipeAction::Encoder(e) => {
                let name = ENCODERS
                    .iter()
                    .find(|(x, _)| *x == e)
                    .map_or("", |(_, n)| n);
                format!("encoder-{name}")
            }
            PipeAction::Strip => "strip".into(),
            PipeAction::Run(id) => format!("run-{id}"),
            PipeAction::Producer => "producer".into(),
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        match key.split_once('-') {
            Some(("encoder", name)) => ENCODERS
                .iter()
                .find(|(_, n)| *n == name)
                .map(|(e, _)| PipeAction::Encoder(*e)),
            Some(("run", id)) => id.parse().ok().map(PipeAction::Run),
            Some(_) => None,
            None => match key {
                "strip" => Some(PipeAction::Strip),
                "producer" => Some(PipeAction::Producer),
                _ => None,
            },
        }
    }
}

/// The pipeline media a card belongs to.
fn media_of(card: &JobUpdate) -> Media {
    if card.video.is_some() {
        Media::Video
    } else if card.audio.is_some() {
        Media::Audio
    } else if card.source_format == Some(Format::Pdf) {
        Media::Pdf
    } else {
        Media::Image
    }
}

fn idle(card: &JobUpdate) -> bool {
    !matches!(card.state, JobState::Queued | JobState::Optimising { .. })
}

/// The edit tools follow `can_edit` (they need the original); metadata is an
/// image matter; "Show producing pipeline" needs only an idle card.
pub fn enabled(card: &JobUpdate, action: PipeAction) -> bool {
    match action {
        PipeAction::Producer => idle(card),
        PipeAction::Strip => card.can_edit && media_of(card) == Media::Image,
        PipeAction::Encoder(_) | PipeAction::Run(_) => card.can_edit,
    }
}

pub struct Items {
    pub encoder: Submenu<Wry>,
    pub strip: CheckMenuItem<Wry>,
    pub run: Submenu<Wry>,
    pub producer: MenuItem<Wry>,
}

impl Items {
    pub fn build(app: &AppHandle, card: &JobUpdate) -> tauri::Result<Items> {
        let id = |action: PipeAction| item_id(&card.id, Action::Pipe(action));
        let checks = ENCODERS
            .iter()
            .map(|(e, name)| {
                CheckMenuItem::with_id(
                    app,
                    id(PipeAction::Encoder(*e)),
                    t(&format!("pipe.encoder.{name}")),
                    card.can_edit,
                    card.edits.encoder == Some(*e),
                    None::<&str>,
                )
            })
            .collect::<tauri::Result<Vec<_>>>()?;
        let refs: Vec<&dyn IsMenuItem<Wry>> =
            checks.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
        let encoder = Submenu::with_items(app, t("pipe.menu.encoder"), card.can_edit, &refs)?;

        let state = app.state::<AppState>();
        let (strip_default, library) = {
            let settings = state.settings.read().unwrap();
            let media = media_of(card);
            let library: Vec<(u32, String)> = settings
                .pipelines
                .library
                .iter()
                .filter(|p| p.media == media)
                .map(|p| (p.id, p.name.clone()))
                .collect();
            (settings.images.strip_metadata, library)
        };
        let strip = CheckMenuItem::with_id(
            app,
            id(PipeAction::Strip),
            t("pipe.menu.strip"),
            enabled(card, PipeAction::Strip),
            card.edits.strip_metadata.unwrap_or(strip_default),
            None::<&str>,
        )?;

        let run_items = if library.is_empty() {
            vec![MenuItem::with_id(
                app,
                "pipe-run-none",
                t("pipe.menu.run_none"),
                false,
                None::<&str>,
            )?]
        } else {
            library
                .iter()
                .map(|(pipeline, name)| {
                    MenuItem::with_id(
                        app,
                        id(PipeAction::Run(*pipeline)),
                        name,
                        card.can_edit,
                        None::<&str>,
                    )
                })
                .collect::<tauri::Result<Vec<_>>>()?
        };
        let run_refs: Vec<&dyn IsMenuItem<Wry>> = run_items
            .iter()
            .map(|i| i as &dyn IsMenuItem<Wry>)
            .collect();
        let run = Submenu::with_items(app, t("pipe.menu.run"), card.can_edit, &run_refs)?;

        let producer = MenuItem::with_id(
            app,
            id(PipeAction::Producer),
            t("pipe.menu.producer"),
            enabled(card, PipeAction::Producer)
                && app.state::<PipeState>().producer(&card.id).is_some(),
            None::<&str>,
        )?;
        Ok(Items {
            encoder,
            strip,
            run,
            producer,
        })
    }
}

/// Do what a pipeline item of the menu says.
pub fn run(app: &AppHandle, id: &JobId, action: PipeAction) -> Result<(), String> {
    match action {
        PipeAction::Encoder(e) => {
            let current = current_edits(app, id)?.encoder;
            jobs::edit(
                app,
                id,
                EditsPatch {
                    encoder: Some((current != Some(e)).then_some(e)),
                    ..EditsPatch::default()
                },
            )
        }
        PipeAction::Strip => {
            let default = app
                .state::<AppState>()
                .settings
                .read()
                .unwrap()
                .images
                .strip_metadata;
            let effective = current_edits(app, id)?.strip_metadata.unwrap_or(default);
            jobs::edit(
                app,
                id,
                EditsPatch {
                    strip_metadata: Some(Some(!effective)),
                    ..EditsPatch::default()
                },
            )
        }
        PipeAction::Run(pipeline) => run_on_card(app, id, pipeline),
        PipeAction::Producer => {
            let producer = app.state::<PipeState>().producer(id);
            pipe_windows::open_editor(app, producer, None);
            Ok(())
        }
    }
}

fn current_edits(app: &AppHandle, id: &JobId) -> Result<ferry_core::edit::Edits, String> {
    let entry = jobs::idle_entry(app, id)?;
    let edits = entry.last.lock().unwrap().edits.clone();
    Ok(edits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done() -> JobUpdate {
        let mut card = ferry_core::pipeline::notice("x".into());
        card.id = serde_json::from_value(serde_json::json!("18f-1")).unwrap();
        card.state = JobState::Done;
        card.can_edit = true;
        card.output = Some("/tmp/a.png".into());
        card
    }

    #[test]
    fn every_pipe_item_id_reads_back() {
        let card = done();
        for p in [
            PipeAction::Encoder(Encoder::Fast),
            PipeAction::Encoder(Encoder::VisuallyLossless),
            PipeAction::Strip,
            PipeAction::Run(12),
            PipeAction::Producer,
        ] {
            assert_eq!(
                crate::menu::parse_id(&item_id(&card.id, Action::Pipe(p))),
                Some((card.id.clone(), Action::Pipe(p)))
            );
        }
        assert_eq!(PipeAction::parse("run-x"), None);
        assert_eq!(PipeAction::parse("encoder-huge"), None);
    }

    #[test]
    fn tools_follow_can_edit_and_producer_follows_idle() {
        let mut card = done();
        card.can_edit = false;
        assert!(
            !enabled(&card, PipeAction::Strip)
                && !enabled(&card, PipeAction::Run(1))
                && enabled(&card, PipeAction::Producer)
        );
        card.state = JobState::Queued;
        assert!(!enabled(&card, PipeAction::Producer));
    }

    #[test]
    fn strip_metadata_is_for_image_cards() {
        let mut card = done();
        assert!(enabled(&card, PipeAction::Strip));
        card.audio = Some(ferry_core::audio::AudioCard {
            bitrate_before: None,
            bitrate_after: None,
            lossless: false,
            has_cover: false,
        });
        assert!(!enabled(&card, PipeAction::Strip) && enabled(&card, PipeAction::Run(1)));
    }
}
