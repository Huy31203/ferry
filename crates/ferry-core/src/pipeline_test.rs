//! The editor's test run: a pipeline on a sample image, step by step, inside a
//! sandbox folder. Nothing outside the sandbox is written, and the sample is
//! only read.

use std::{fs, path::Path, time::Instant};

use chrono::NaiveDate;
use serde::Serialize;

use crate::{
    edit::{self, Edits, raster},
    i18n::{t, tf},
    optimise::{self, OptimiseOptions, Outcome},
    pipeline_after::copy_target,
    pipeline_def::Pipeline,
    pipeline_plan::{captions, compile},
    pipeline_steps::{Effect, Facts, Media, Registry},
    settings::contract_home,
    sniff::{self, Format},
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepReport {
    /// 0-based.
    pub step: usize,
    pub caption: String,
    pub bytes: Option<u64>,
    pub dims: Option<(u32, u32)>,
    pub millis: u64,
    pub error: Option<String>,
    pub note: Option<String>,
}

/// The latest result of the run so far.
struct Latest {
    bytes: Vec<u8>,
    format: Format,
    dims: (u32, u32),
}

/// Run `p` on `sample` inside `sandbox` (emptied first). `Err` only when the
/// sample cannot be read or is not an image.
pub fn run(
    p: &Pipeline,
    r: &Registry,
    sample: &Path,
    sandbox: &Path,
    o: &OptimiseOptions,
    home: &Path,
    today: NaiveDate,
) -> Result<Vec<StepReport>, String> {
    let name = sample
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let not_image = || tf("pipe.test.not_image", &[("name", &name)]);
    let original = fs::read(sample).map_err(|_| not_image())?;
    let source = sniff::sniff(&original)
        .filter(|f| matches!(f, Format::Png | Format::Jpeg | Format::WebP | Format::Gif))
        .ok_or_else(not_image)?;
    let dims = raster::display_dims(&original, source).ok_or_else(not_image)?;
    if sandbox.exists() {
        fs::remove_dir_all(sandbox).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(sandbox).map_err(|e| e.to_string())?;

    let facts = Facts {
        stem: sample
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        ext: sample
            .extension()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        media: Media::Image,
    };
    let mut latest = Latest {
        bytes: original.clone(),
        format: source,
        dims,
    };
    let names = captions(p, r);
    let mut reports = Vec::new();
    for (i, step) in p.steps.iter().enumerate() {
        let mut report = StepReport {
            step: i,
            caption: names[i].clone(),
            bytes: None,
            dims: None,
            millis: 0,
            error: None,
            note: None,
        };
        let started = Instant::now();
        let Some(kind) = r.get(p.media, &step.kind) else {
            report.error = compile(&prefix(p, i), r, &facts).err().map(|e| e.message);
            reports.push(report);
            break;
        };
        // A fork closes a branch; the test follows the main one.
        if matches!(kind.effect, Effect::Fork) {
            report.note = Some(t("pipe.caption.fork"));
            reports.push(report);
            continue;
        }
        let compiled = match compile(&prefix(p, i), r, &facts) {
            Ok(c) => c,
            Err(issue) => {
                report.error = Some(issue.message);
                reports.push(report);
                break;
            }
        };
        match kind.effect {
            Effect::Guard(_) => {
                if compiled.stopped_at == Some(i) {
                    let text = step
                        .params
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    report.note = Some(tf("pipe.test.skipped", &[("text", text)]));
                    reports.push(report);
                    break;
                }
            }
            Effect::Fold(_) if kind.phase == crate::pipeline_steps::Phase::Output => {
                // Only copies exist as output steps: show where it would go, in the sandbox.
                let result_name = format!("{}.{}", facts.stem, latest.format.extension());
                let shown = copy_target(
                    &template_of(&compiled.main, i),
                    Path::new(&result_name),
                    latest.dims,
                    home,
                    today,
                );
                if let Some(file) = shown.file_name() {
                    let _ = fs::write(sandbox.join(file), &latest.bytes);
                }
                report.note = Some(tf(
                    "pipe.test.would_copy",
                    &[("path", &contract_home(&shown, home))],
                ));
                report.bytes = Some(latest.bytes.len() as u64);
                report.dims = Some(latest.dims);
            }
            Effect::Fold(_) => {
                match apply(&original, source, &compiled.main.recipe, o) {
                    Ok(done) => latest = done,
                    Err(e) => {
                        report.error = Some(e);
                        report.millis = started.elapsed().as_millis() as u64;
                        reports.push(report);
                        break;
                    }
                }
                report.bytes = Some(latest.bytes.len() as u64);
                report.dims = Some(latest.dims);
            }
            Effect::Fork => {}
        }
        report.millis = started.elapsed().as_millis() as u64;
        reports.push(report);
    }
    Ok(reports)
}

/// `p` cut after step `i`.
fn prefix(p: &Pipeline, i: usize) -> Pipeline {
    Pipeline {
        steps: p.steps[..=i].to_vec(),
        ..p.clone()
    }
}

/// The template of the copy step at `step`, from the compiled branch.
fn template_of(branch: &crate::pipeline_steps::Branch, step: usize) -> String {
    branch
        .after
        .iter()
        .find_map(|a| match a {
            crate::pipeline_steps::After::CopyTo { template, step: s } if *s == step => {
                Some(template.clone())
            }
            _ => None,
        })
        .unwrap_or_default()
}

/// The recipe applied to the original, as a card edit would.
fn apply(
    original: &[u8],
    source: Format,
    recipe: &Edits,
    o: &OptimiseOptions,
) -> Result<Latest, String> {
    if recipe.is_plain() {
        let opts = OptimiseOptions {
            level: recipe.level,
            encoder: recipe.encoder,
            ..o.clone()
        };
        let bytes = match optimise::optimise(original, source, &opts).map_err(|e| e.to_string())? {
            Outcome::Smaller { bytes } => bytes,
            _ => original.to_vec(),
        };
        let dims = raster::display_dims(&bytes, source).unwrap_or_default();
        return Ok(Latest {
            bytes,
            format: source,
            dims,
        });
    }
    let done = edit::apply(original, source, recipe, o).map_err(|e| e.to_string())?;
    Ok(Latest {
        bytes: done.bytes,
        format: done.format,
        dims: done.dims,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{pipeline_steps::builtin, testutil};
    use serde_json::json;

    #[test]
    fn a_test_run_reports_each_step_and_never_touches_the_sample_or_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let sample = dir.path().join("invoice.png");
        let original = testutil::photo_png(200, 100);
        fs::write(&sample, &original).unwrap();
        let target = dir.path().join("target");
        let p = Pipeline {
            media: Media::Image,
            steps: serde_json::from_value(json!([
                { "kind": "downscale", "params": { "by": "percent", "value": 50 } },
                { "kind": "copy", "params": { "to": format!("{}/%f.%e", target.display()) } },
                { "kind": "if_name_contains", "params": { "text": "receipt" } },
                { "kind": "convert", "params": { "to": "webp" } }
            ]))
            .unwrap(),
            ..Default::default()
        };
        let sandbox = dir.path().join("sandbox");
        let reports = run(
            &p,
            &builtin(),
            &sample,
            &sandbox,
            &OptimiseOptions::default(),
            dir.path(),
            NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
        )
        .unwrap();
        assert_eq!(reports.len(), 3);
        assert_eq!(reports[0].dims, Some((100, 50)));
        assert!(
            reports[1]
                .note
                .as_deref()
                .unwrap()
                .starts_with("Would copy to")
        );
        assert!(sandbox.join("invoice.png").exists() && !target.exists());
        assert!(reports[2].note.as_deref().unwrap().contains("receipt"));
        assert_eq!(fs::read(&sample).unwrap(), original);
    }

    #[test]
    fn an_issue_ends_the_run_and_a_non_image_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let sample = dir.path().join("a.png");
        fs::write(&sample, testutil::png(64, 64)).unwrap();
        let p = Pipeline {
            media: Media::Image,
            steps: serde_json::from_value(json!([
                { "kind": "strip_exif" },
                { "kind": "teleport" },
                { "kind": "strip_exif" }
            ]))
            .unwrap(),
            ..Default::default()
        };
        let today = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let o = OptimiseOptions::default();
        let sandbox = dir.path().join("sb");
        let reports = run(&p, &builtin(), &sample, &sandbox, &o, dir.path(), today).unwrap();
        assert_eq!(reports.len(), 2);
        assert!(
            reports[0].error.is_none() && reports[1].error.as_deref().unwrap().contains("teleport")
        );
        let text = dir.path().join("notes.txt");
        fs::write(&text, b"hello").unwrap();
        let err = run(&p, &builtin(), &text, &sandbox, &o, dir.path(), today).unwrap_err();
        assert!(err.contains("notes.txt"));
    }
}
