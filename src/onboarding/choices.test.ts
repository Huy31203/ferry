import { describe, expect, it } from 'vitest';
import { sample } from '../lib/settings.fixture';
import { applyChoices, fromSettings } from './choices';

const base = structuredClone(sample);

describe('onboarding choices', () => {
  it('start from the settings', () => {
    const c = fromSettings(base);
    expect(c).toEqual({ keepOriginal: false, quality: 95, encoder: base.video.encoder, archive: 'replace' });
  });

  it('keep the original sets both optimised locations to same_folder and marks onboarded', () => {
    const initial = fromSettings(base);
    const next = applyChoices(base, initial, { ...initial, keepOriginal: true, quality: 70, encoder: 'small', archive: 'add' }, true);
    expect(next.images.output.optimised.location).toBe('same_folder');
    expect(next.video.output.optimised.location).toBe('same_folder');
    expect(next.video.compression).toBe(70);
    expect(next.video.encoder).toBe('small');
    expect(next.floating.shelf_archive_result).toBe('add');
    expect(next.general.onboarded).toBe(true);
    expect(base.general.onboarded).toBe(false);
  });

  it('an_untouched_radio_keeps_a_custom_location', () => {
    const custom = structuredClone(base);
    custom.images.output.optimised.location = 'folder';
    const initial = fromSettings(custom);
    expect(initial.keepOriginal).toBe(false);
    const next = applyChoices(custom, initial, initial, true);
    expect(next.images.output.optimised.location).toBe('folder');
  });

  it('leaves the shelf setting alone without a shelf', () => {
    const initial = fromSettings(base);
    const next = applyChoices(base, initial, { ...initial, archive: 'add' }, false);
    expect(next.floating.shelf_archive_result).toBe('replace');
  });
});
