import { describe, expect, it } from 'vitest';
import type { Summary, ZoneView } from './drop-ipc';
import { kindsText, zoneText } from './zone';

const none: Summary = {
  files: 0,
  folders: 0,
  images: 0,
  videos: 0,
  audio: 0,
  pdfs: 0,
  unsupported: 0,
  unsupported_ext: null,
};

const view = (summary: Partial<Summary>, over: Partial<ZoneView> = {}): ZoneView => ({
  state: 'hover',
  mode: 'zone',
  summary: { ...none, ...summary },
  keep_original: false,
  opens_batch: false,
  ...over,
});

describe('zone text', () => {
  it('names what is dragged like the spec', () => {
    expect(kindsText({ ...none, files: 4, images: 3, videos: 1 })).toBe('3 images, 1 video');
    expect(kindsText({ ...none, files: 1, images: 1 })).toBe('1 image');
  });

  it('hints at the batch for many files or a folder', () => {
    expect(zoneText(view({ files: 42, images: 42 }, { opens_batch: true })).sub).toBe(
      '42 files · opens Batch',
    );
    expect(zoneText(view({ folders: 1 }, { opens_batch: true })).sub).toBe('1 folder · opens Batch');
  });

  it('dims and names an unsupported file', () => {
    const text = zoneText(
      view({ files: 1, unsupported: 1, unsupported_ext: 'zip' }, { state: 'unsupported' }),
    );
    expect([text.label, text.dim]).toEqual(["Can't optimise .zip", true]);
  });

  it('shows the keep line only while the modifier is held', () => {
    expect(zoneText(view({ files: 1, images: 1 })).keep).toBeNull();
    expect(zoneText(view({ files: 1, images: 1 }, { keep_original: true })).keep).toBe(
      'Originals will be kept',
    );
  });

  it('counts files not read yet as files', () => {
    expect(kindsText({ ...none, files: 30, images: 2 })).toBe('2 images, 28 files');
  });
});
