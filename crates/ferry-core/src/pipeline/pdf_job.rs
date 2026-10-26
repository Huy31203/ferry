//! PDF jobs end to end (sE pdf-formats): what the file is, the refusals
//! (encrypted, signed), the structural optimise and the card's line. The
//! result is placed by the same `put` / `put_back` rules as an image's, under
//! the PDF output rules.

use super::{Content, Deliver, Finished, Job, RunOptions, bytes_label};
use crate::i18n::{t, tf};
use crate::job::{JobState, JobUpdate};
use crate::optimise::{self, Level, OptimiseError, OptimiseOptions, Outcome};
use crate::pdf::{self, PdfResult};
use crate::sniff::Format;

impl Job {
    /// Read a PDF's page count and size on the first run; refuse an encrypted
    /// or unreadable file. Any other job passes.
    pub(super) fn capture_facts(&mut self, original: &[u8]) -> Result<(), String> {
        if self.source_format != Format::Pdf || self.pages.is_some() {
            return Ok(());
        }
        match pdf::facts(original) {
            Ok(facts) if facts.encrypted => Err(t("pdf.error.encrypted")),
            Ok(facts) => {
                self.pages = Some(facts.pages);
                self.dims = Some(facts.size);
                Ok(())
            }
            Err(_) => Err(t("pdf.error.read")),
        }
    }

    /// Optimise or edit a PDF original and write the result. A plain run keeps
    /// Cycle 1's never-worse rule; an edit always writes.
    pub(super) fn finish_pdf(
        &mut self,
        original: &[u8],
        options: &OptimiseOptions,
        o: &RunOptions,
        emit: &mut dyn FnMut(JobUpdate),
        deliver: Deliver,
    ) -> Option<Finished> {
        let extracting = matches!(self.edits.format, Some(Format::Png | Format::Jpeg));
        if !extracting {
            // Rewriting a signed file would break its signature.
            match pdf::facts(original) {
                Ok(facts) if facts.signed => {
                    let reason = t("pdf.error.signed");
                    return Some(self.fail(reason.clone(), reason));
                }
                Ok(_) => {}
                Err(e) => return Some(self.fail(self.reason(&e), e)),
            }
        }
        let plain = self.edits.is_plain() && !self.twin;
        let dpi = pdf::target_dpi(&self.edits, &o.pdf, extracting);
        let step = t("pdf.step.pdf");
        let mut last = 0.0_f32;
        let job: &Job = self;
        let mut report = |progress: f32, step: String| {
            if progress >= 1.0 || progress - last >= 0.02 {
                last = progress;
                emit(job.update(JobState::Optimising { progress, step }, None));
            }
        };
        // (state, Some((bytes, fit target missed)))
        let (state, result) = if plain {
            match pdf::optimise(original, dpi, options, &mut |p| report(p, step.clone())) {
                Ok(bytes) => match optimise::classify(original.len(), bytes) {
                    Outcome::Smaller { bytes } => (JobState::Done, Some((bytes, false))),
                    Outcome::AlreadyOptimal => (JobState::AlreadyOptimal, None),
                    Outcome::Larger => (JobState::Larger, None),
                },
                Err(OptimiseError::Cancelled) => return None,
                Err(e) => return Some(self.fail(self.reason(&e), e)),
            }
        } else {
            match pdf::apply(original, &self.edits, options, &o.pdf, &mut report) {
                Ok(PdfResult::Pdf {
                    bytes,
                    missed_target,
                }) => (JobState::Done, Some((bytes, missed_target))),
                Ok(PdfResult::Pages { .. }) => {
                    let e = OptimiseError::Unsupported(tf(
                        "pdf.write_unsupported",
                        &[("format", self.edits.format.map_or("", Format::label))],
                    ));
                    return Some(self.fail(self.reason(&e), e));
                }
                Err(OptimiseError::Cancelled) => return None,
                Err(e) => return Some(self.fail(self.reason(&e), e)),
            }
        };
        // The last moment a cancel counts: after this the user's data changes.
        if o.cancel.is_cancelled() {
            return None;
        }
        self.pdf_dpi = Some(dpi);
        self.dims_after = None;
        let written = match result {
            Some((bytes, missed)) => {
                self.note = missed.then(|| {
                    let target = bytes_label(self.edits.fit_under.unwrap_or_default());
                    tf("card.note.fit_missed", &[("size", &target)])
                });
                let after = bytes.len() as u64;
                self.put(
                    Content::Bytes(&bytes),
                    Format::Pdf,
                    Content::Bytes(original),
                    o,
                    deliver,
                )
                .map(|()| Some(after))
            }
            None => {
                self.note = None;
                let replace = self.changed;
                self.put_back(Content::Bytes(original), replace, o, deliver)
                    .map(|()| None)
            }
        };
        Some(match written {
            Ok(after) => Finished {
                update: self.update(state, after),
                cause: None,
            },
            Err((reason, cause)) => self.fail(reason, cause),
        })
    }

    /// "3 pages · 150 dpi", plus "cropped" and "aggressive" when they apply.
    pub(super) fn pdf_detail(&self) -> String {
        let pages = self.pages.unwrap_or(1);
        let mut parts = vec![tf(
            if pages == 1 {
                "pdf.detail.page"
            } else {
                "pdf.detail.pages"
            },
            &[("n", &pages.to_string())],
        )];
        if let Some(dpi) = self.pdf_dpi {
            parts.push(tf("pdf.detail.dpi", &[("n", &dpi.to_string())]));
        }
        if self.edits.aspect.is_some() || self.edits.uncrop {
            parts.push(t("pdf.detail.cropped"));
        }
        if self.level == Level::Aggressive {
            parts.push(t("card.detail.aggressive"));
        }
        parts.join(" · ")
    }
}
