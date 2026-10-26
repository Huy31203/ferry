//! What a job does for the formats of stream E that its image engine does
//! not cover alone: posters for originals the webviews cannot draw.

use super::Job;
use crate::backup::Backups;
use crate::poster;

impl Job {
    /// Store a PNG of the original beside its backup, once, when the card's
    /// thumbnail cannot be the backup itself. A failure only leaves the card
    /// without a poster; it is never a job error.
    pub(super) fn capture_poster(&mut self, original: &[u8], backups: &Backups) {
        if self.poster.is_some() || !poster::needs_poster(self.source_format) {
            return;
        }
        let Ok((png, _)) = poster::png(original, self.source_format, 1, poster::CARD_EDGE) else {
            return;
        };
        self.poster = backups
            .store_bytes(self.id.as_str(), ".ferry-poster.png", &png)
            .ok();
    }
}
