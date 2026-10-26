import { describe, expect, it } from 'vitest';
import { duplicate, newPipeline, removePipeline, usage, usageLabel, type Pipelines } from './pipelines';

const none = { image: null, video: null, audio: null, pdf: null };
const empty = (): Pipelines => ({ library: [], clipboard: { ...none }, drop_zone: { ...none }, zones: [] });

describe('pipelines', () => {
  it('gives a new pipeline the next id', () => {
    const a = newPipeline(empty(), 'image', 'A');
    const b = newPipeline(a.lib, 'image', 'B');
    expect([a.id, b.id]).toEqual([1, 2]);
  });
  it('duplicates steps under a new id and name', () => {
    const a = newPipeline(empty(), 'image', 'A', [{ kind: 'convert', params: { to: 'webp' } }]);
    const d = duplicate(a.lib, a.id, 'A copy');
    expect(d.lib.library[1]).toMatchObject({ id: 2, name: 'A copy', steps: [{ kind: 'convert', params: { to: 'webp' } }] });
    expect(d.lib.library[1].steps).not.toBe(d.lib.library[0].steps);
  });
  it('removing a pipeline clears every assignment', () => {
    const a = newPipeline(empty(), 'image', 'A');
    const lib: Pipelines = {
      ...a.lib,
      clipboard: { ...none, image: 1 },
      drop_zone: { ...none, image: 1 },
      zones: [{ id: 1, name: 'Z', icon: 'image', pipelines: { ...none, image: 1 } }],
    };
    const after = removePipeline(lib, 1);
    expect([after.library.length, after.clipboard.image, after.drop_zone.image, after.zones[0].pipelines.image]).toEqual([
      0,
      null,
      null,
      null,
    ]);
  });
  it('lists where a pipeline is used', () => {
    const lib: Pipelines = {
      ...newPipeline(empty(), 'image', 'A').lib,
      clipboard: { ...none, image: 1 },
      zones: [{ id: 3, name: 'WebP', icon: 'image', pipelines: { ...none, image: 1 } }],
    };
    expect(usage(lib, 1)).toEqual([
      { kind: 'clipboard', media: 'image' },
      { kind: 'zone', media: 'image', zone: 'WebP' },
    ]);
    expect(usageLabel(usage(lib, 1))).toBe('Copied images · Zone “WebP”');
    expect(usageLabel([])).toBe('Not assigned to anything yet.');
  });
});
