//! The stored pipelines (`[pipelines]` in settings): ordered steps per
//! pipeline, where each one runs, and the preset zones. Loading is lenient so a
//! file from a newer Ferry never resets the rest of the user's settings.

use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};

use crate::{
    i18n::t,
    pipeline_steps::{Media, Params},
};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Step {
    pub kind: String,
    pub params: Params,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Pipeline {
    pub id: u32,
    pub name: String,
    pub media: Media,
    pub steps: Vec<Step>,
    pub hide_result: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One pipeline id (or none) per media type.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PerMedia {
    pub image: Option<u32>,
    pub video: Option<u32>,
    pub audio: Option<u32>,
    pub pdf: Option<u32>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl PerMedia {
    pub fn get(&self, m: Media) -> Option<u32> {
        match m {
            Media::Image => self.image,
            Media::Video => self.video,
            Media::Audio => self.audio,
            Media::Pdf => self.pdf,
            Media::Other => None,
        }
    }

    /// `Media::Other` has no slot; setting it does nothing.
    pub fn set(&mut self, m: Media, id: Option<u32>) {
        match m {
            Media::Image => self.image = id,
            Media::Video => self.video = id,
            Media::Audio => self.audio = id,
            Media::Pdf => self.pdf = id,
            Media::Other => {}
        }
    }
}

pub const ZONE_ICONS: [&str; 6] = ["image", "copy", "folder", "zap", "globe", "star"];

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Zone {
    pub id: u32,
    pub name: String,
    pub icon: String,
    pub pipelines: PerMedia,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Pipelines {
    #[serde(deserialize_with = "lenient")]
    pub library: Vec<Pipeline>,
    /// The pipeline a copied file runs, per media.
    pub clipboard: PerMedia,
    pub drop_zone: PerMedia,
    #[serde(deserialize_with = "lenient")]
    pub zones: Vec<Zone>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A JSON array whose elements that fail to parse are dropped; anything that
/// is not an array is empty.
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Vec<T>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| serde_json::from_value(item).ok())
            .collect(),
        _ => Vec::new(),
    })
}

/// Keep each id the first time it is seen; 0 and repeats get the next free one.
fn unique_ids(ids: &mut [&mut u32]) {
    let mut seen = HashSet::new();
    let mut redo = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        if **id == 0 || !seen.insert(**id) {
            redo.push(i);
        }
    }
    let first_free = seen.iter().max().map_or(1, |m| m + 1);
    for (i, id) in redo.into_iter().zip(first_free..) {
        *ids[i] = id;
    }
}

impl Pipelines {
    pub fn get(&self, id: u32) -> Option<&Pipeline> {
        self.library.iter().find(|p| p.id == id)
    }

    pub fn zone(&self, id: u32) -> Option<&Zone> {
        self.zones.iter().find(|z| z.id == id)
    }

    /// 1 + the largest library id (1 when empty).
    pub fn next_id(&self) -> u32 {
        self.library.iter().map(|p| p.id).max().map_or(1, |m| m + 1)
    }

    pub fn next_zone_id(&self) -> u32 {
        self.zones.iter().map(|z| z.id).max().map_or(1, |m| m + 1)
    }

    /// Ids 0 or repeated get the next free id; blank names become a default;
    /// unknown zone icons become "image"; every assignment to a missing
    /// pipeline, or to one of another media, is cleared. Steps are never touched.
    pub fn sanitised(mut self) -> Self {
        unique_ids(
            &mut self
                .library
                .iter_mut()
                .map(|p| &mut p.id)
                .collect::<Vec<_>>(),
        );
        unique_ids(&mut self.zones.iter_mut().map(|z| &mut z.id).collect::<Vec<_>>());
        for p in &mut self.library {
            if p.name.trim().is_empty() {
                p.name = t("pipe.untitled");
            }
        }
        for z in &mut self.zones {
            if z.name.trim().is_empty() {
                z.name = t("pipe.zones.untitled");
            }
            if !ZONE_ICONS.contains(&z.icon.as_str()) {
                z.icon = "image".into();
            }
        }
        let valid: HashSet<(u32, Media)> = self.library.iter().map(|p| (p.id, p.media)).collect();
        let clear = |slots: &mut PerMedia| {
            for m in Media::ALL {
                if let Some(id) = slots.get(m)
                    && !valid.contains(&(id, m))
                {
                    slots.set(m, None);
                }
            }
        };
        clear(&mut self.clipboard);
        clear(&mut self.drop_zone);
        for z in &mut self.zones {
            clear(&mut z.pipelines);
        }
        self
    }
}

fn step(kind: &str, params: Value) -> Step {
    Step {
        kind: kind.into(),
        params: params.as_object().cloned().unwrap_or_default(),
        ..Step::default()
    }
}

/// Starter templates for the empty library (id 0: the caller assigns one).
pub fn starters() -> Vec<Pipeline> {
    let make = |name_key: &str, steps: Vec<Step>| Pipeline {
        name: t(name_key),
        media: Media::Image,
        steps,
        ..Pipeline::default()
    };
    vec![
        make(
            "pipe.starter.webp",
            vec![step("convert", json!({ "to": "webp" }))],
        ),
        make(
            "pipe.starter.web",
            vec![
                step("downscale", json!({ "by": "long_edge", "value": 1920 })),
                step("optimise", json!({})),
            ],
        ),
        make(
            "pipe.starter.invoices",
            vec![
                step("if_name_contains", json!({ "text": "invoice" })),
                step("optimise", json!({})),
                step("copy", json!({ "to": "~/Documents/Invoices/%f.%e" })),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{LoadStatus, Settings};
    use std::fs;

    #[test]
    fn the_defaults_are_empty() {
        let p = Pipelines::default();
        assert!(p.library.is_empty() && p.zones.is_empty());
        assert_eq!((p.clipboard.image, p.drop_zone.image), (None, None));
    }

    #[test]
    fn a_pipeline_from_a_newer_ferry_survives_a_load_and_save() {
        let json = json!({ "library": [{ "id": 4, "name": "Future", "media": "hologram", "colour": "teal",
            "steps": [{ "kind": "teleport", "params": { "to": "mars" }, "note": "x" }] }] });
        let p: Pipelines = serde_json::from_value(json).unwrap();
        assert_eq!(p.library[0].media, Media::Other);
        let back = serde_json::to_value(&p).unwrap();
        assert_eq!(back["library"][0]["colour"], "teal");
        assert_eq!(
            back["library"][0]["steps"][0],
            json!({ "kind": "teleport", "params": { "to": "mars" }, "note": "x" })
        );
    }

    #[test]
    fn a_broken_pipeline_is_dropped_not_the_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            json!({ "version": 2, "images": { "compression": 55 },
                "pipelines": { "library": [{ "id": "not a number" }, { "id": 2, "name": "Ok", "steps": [] }] } })
            .to_string(),
        )
        .unwrap();
        let (s, status) = Settings::load(&path);
        assert!(matches!(status, LoadStatus::Loaded));
        assert_eq!(s.images.compression, 55);
        assert_eq!(
            s.pipelines.library.iter().map(|p| p.id).collect::<Vec<_>>(),
            [2]
        );
    }

    #[test]
    fn sanitise_gives_duplicate_ids_new_ones_and_clears_dangling_assignments() {
        let mut p = Pipelines {
            library: vec![
                Pipeline {
                    id: 1,
                    ..Default::default()
                },
                Pipeline {
                    id: 1,
                    ..Default::default()
                },
                Pipeline {
                    id: 0,
                    ..Default::default()
                },
            ],
            zones: vec![Zone {
                id: 1,
                pipelines: PerMedia {
                    image: Some(9),
                    ..Default::default()
                },
                icon: "rocket".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        p.clipboard.image = Some(9);
        let p = p.sanitised();
        assert_eq!(
            p.library.iter().map(|p| p.id).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            (
                p.clipboard.image,
                p.zones[0].pipelines.image,
                p.zones[0].icon.as_str()
            ),
            (None, None, "image")
        );
        assert_eq!(p.library[0].name, t("pipe.untitled"));
    }

    #[test]
    fn an_assignment_to_a_pipeline_of_another_media_is_cleared() {
        let mut p = Pipelines {
            library: vec![Pipeline {
                id: 1,
                name: "v".into(),
                media: Media::Video,
                ..Default::default()
            }],
            ..Default::default()
        };
        p.clipboard.image = Some(1);
        p.clipboard.video = Some(1);
        let p = p.sanitised();
        assert_eq!((p.clipboard.image, p.clipboard.video), (None, Some(1)));
    }

    #[test]
    fn ids_continue_after_the_largest() {
        let mut p = Pipelines::default();
        assert_eq!((p.next_id(), p.next_zone_id()), (1, 1));
        p.library.push(Pipeline {
            id: 7,
            ..Default::default()
        });
        assert_eq!(p.next_id(), 8);
    }
}
