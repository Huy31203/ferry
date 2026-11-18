import type { Settings } from '../lib/settings';

export type Encoder = Settings['video']['encoder'];
export type Choices = { keepOriginal: boolean; quality: number; encoder: Encoder; archive: 'replace' | 'add' };

export function fromSettings(s: Settings): Choices {
  return {
    keepOriginal: [s.images.output.optimised.location, s.video.output.optimised.location].includes('same_folder'),
    quality: s.video.compression,
    encoder: s.video.encoder,
    archive: s.floating.shelf_archive_result,
  };
}

/** The settings to save on Done. The location is only written when the radio was changed (spec §4). */
export function applyChoices(s: Settings, initial: Choices, c: Choices, includeShelf: boolean): Settings {
  const next = structuredClone(s);
  if (c.keepOriginal !== initial.keepOriginal) {
    const location = c.keepOriginal ? 'same_folder' : 'in_place';
    next.images.output.optimised.location = location;
    next.video.output.optimised.location = location;
  }
  next.video.compression = c.quality;
  next.video.encoder = c.encoder;
  if (includeShelf) next.floating.shelf_archive_result = c.archive;
  next.general.onboarded = true;
  return next;
}
