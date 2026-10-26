import { describe, expect, it } from 'vitest';
import {
  addStep,
  applySuggestion,
  duplicate,
  moveStep,
  newPipeline,
  removePipeline,
  removeStep,
  setParam,
  suggest,
  usage,
  usageLabel,
  type Pipelines,
  type StepKindView,
} from './pipelines';

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

const kinds: StepKindView[] = [
  {
    id: 'convert',
    media: 'image',
    label: 'pipe.step.convert',
    phase: 'transform',
    defaults: { to: 'webp' },
    params: [
      {
        name: 'to',
        label: 'pipe.param.convert_to',
        required: true,
        shown_when: null,
        control: {
          kind: 'choice',
          options: [
            { value: 'png', label: 'pipe.choice.png' },
            { value: 'webp', label: 'pipe.choice.webp' },
          ],
        },
      },
    ],
  },
  {
    id: 'copy',
    media: 'image',
    label: 'pipe.step.copy',
    phase: 'output',
    defaults: {},
    params: [{ name: 'to', label: 'pipe.param.copy_to', required: true, shown_when: null, control: { kind: 'template' } }],
  },
  { id: 'crop', media: 'image', label: 'pipe.step.crop', phase: 'transform', defaults: { aspect: '16:9', smart: true }, params: [] },
];

describe('steps', () => {
  it('moves, adds with defaults and removes', () => {
    const s = addStep(addStep([], kinds[0]), kinds[1], 0);
    expect(s.map((x) => x.kind)).toEqual(['copy', 'convert']);
    expect(s[1].params).toEqual({ to: 'webp' });
    expect(moveStep(s, 0, 1).map((x) => x.kind)).toEqual(['convert', 'copy']);
    expect(removeStep(s, 0).map((x) => x.kind)).toEqual(['convert']);
  });
  it('deletes a param set to undefined', () => {
    expect(setParam([{ kind: 'convert', params: { to: 'png' } }], 0, 'to', undefined)[0].params).toEqual({});
    expect(setParam([{ kind: 'convert', params: {} }], 0, 'to', 'png')[0].params).toEqual({ to: 'png' });
  });
});

describe('suggest', () => {
  it('offers step ids, then params, then choice values', () => {
    expect(suggest('c', 1, kinds)).toEqual(['convert', 'copy', 'crop']);
    expect(suggest('convert ', 8, kinds)).toEqual(['to=']);
    expect(suggest('convert to=w', 12, kinds)).toEqual(['webp']);
    expect(suggest('convert to=webp ', 16, kinds)).toEqual([]);
  });
  it('inserts a suggestion in place of the word at the cursor', () => {
    expect(applySuggestion('co', 2, 'convert')).toEqual({ line: 'convert ', col: 8 });
    expect(applySuggestion('convert t', 9, 'to=')).toEqual({ line: 'convert to=', col: 11 });
    expect(applySuggestion('convert to=w', 12, 'webp')).toEqual({ line: 'convert to=webp ', col: 16 });
  });
});
