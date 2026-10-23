import { describe, expect, it } from 'vitest';
import { card } from './card.fixture';
import {
  QUALITY_STOPS,
  convertRequest,
  formatChoices,
  SCALE_STOPS,
  currentQuality,
  nextQuality,
  nextScale,
  qualityLabel,
  qualityPatch,
  scaleLabel,
  scalePatch,
  sliderIndex,
  stopIndex,
} from './edits';

const edits = card({ kind: 'done' }).edits;

describe('downscale steps', () => {
  it('has 100 on top, then 90 to 10 in fives', () => {
    expect(SCALE_STOPS[0]).toBe(100);
    expect(SCALE_STOPS[1]).toBe(90);
    expect(SCALE_STOPS.at(-1)).toBe(10);
    expect(SCALE_STOPS).toHaveLength(18);
  });

  it('clicks down ten points and stops at 10', () => {
    expect(nextScale(null)).toBe(90);
    expect(nextScale(90)).toBe(80);
    expect(nextScale(15)).toBe(10);
    expect(nextScale(10)).toBe(10);
  });

  it('sends a percentage, or clears it at 100', () => {
    expect(scalePatch(50)).toEqual({ scale: 50 });
    expect(scalePatch(100)).toEqual({ scale: null });
  });

  it('labels the live target with the result size', () => {
    expect(scaleLabel(50, [2880, 1800], edits)).toBe('50 % · 1440×900');
    expect(scaleLabel(10, [3, 3], edits)).toBe('10 % · 1×1');
    expect(scaleLabel(50, null, edits)).toBe('50 %');
    expect(scaleLabel(50, [2880, 1800], { ...edits, aspect: [16, 9] })).toBe('50 %');
  });
});

describe('compression steps', () => {
  it('runs from 100 to 5 in fives, then Extreme', () => {
    expect(QUALITY_STOPS[0]).toBe(100);
    expect(QUALITY_STOPS.at(-2)).toBe(5);
    expect(QUALITY_STOPS.at(-1)).toBe('extreme');
    expect(QUALITY_STOPS).toHaveLength(21);
  });

  it('starts from the card, else from the settings', () => {
    expect(currentQuality(edits, 80)).toBe(80);
    expect(currentQuality({ ...edits, compression: 60 }, 80)).toBe(60);
    expect(currentQuality({ ...edits, compression: 5, level: 'aggressive' }, 80)).toBe('extreme');
  });

  it('clicks down ten, then Extreme, and stays there', () => {
    expect(nextQuality(80)).toBe(70);
    expect(nextQuality(15)).toBe(5);
    expect(nextQuality(10)).toBe('extreme');
    expect(nextQuality('extreme')).toBe('extreme');
  });

  it('sends Extreme as aggressive at quality 5', () => {
    expect(qualityPatch('extreme')).toEqual({ compression: 5, level: 'aggressive' });
    expect(qualityPatch(60)).toEqual({ compression: 60, level: 'normal' });
  });

  it('labels quality and Extreme', () => {
    expect(qualityLabel(60)).toBe('q 60');
    expect(qualityLabel('extreme')).toBe('Extreme');
  });
});

describe('slider drag', () => {
  it('moves one stop per step of the track and stays in range', () => {
    expect(sliderIndex(0, 0, 18, 136)).toBe(0);
    expect(sliderIndex(0, 8, 18, 136)).toBe(1);
    expect(sliderIndex(5, -400, 18, 136)).toBe(0);
    expect(sliderIndex(5, 400, 18, 136)).toBe(17);
  });

  it('finds where a value sits, or the stop just below it', () => {
    expect(stopIndex(SCALE_STOPS, 50)).toBe(9);
    expect(stopIndex(SCALE_STOPS, 100)).toBe(0);
    expect(stopIndex(QUALITY_STOPS, 'extreme')).toBe(20);
    expect(stopIndex(QUALITY_STOPS, 82)).toBe(4);
  });
});

describe('format bar', () => {
  const pick = (choices: ReturnType<typeof formatChoices>) =>
    choices.map((c) => [c.label, c.current, c.firstFrame]);

  it('offers PNG, JPEG and WebP with the current one selected', () => {
    expect(pick(formatChoices(card({ kind: 'done' })))).toEqual([
      ['PNG', true, false],
      ['JPEG', false, false],
      ['WebP', false, false],
    ]);
  });

  it('puts a GIF first and marks the others first frame only', () => {
    const gif = card({ kind: 'done' }, { format: 'gif', source_format: 'gif', ext: 'gif' });
    expect(pick(formatChoices(gif))).toEqual([
      ['GIF', true, false],
      ['PNG', false, true],
      ['JPEG', false, true],
      ['WebP', false, true],
    ]);
  });

  it('keeps the GIF chip after a GIF was converted, so it can go back', () => {
    const converted = card({ kind: 'done' }, { format: 'webp', source_format: 'gif', converted: true });
    expect(formatChoices(converted).map((c) => c.format)).toEqual(['gif', 'png', 'jpeg', 'webp']);
    expect(formatChoices(converted).find((c) => c.current)?.format).toBe('webp');
  });

  it('ignores a plain click on the current format but keeps both on ⌥-click', () => {
    const png = card({ kind: 'done' });
    expect(convertRequest(png, 'png', false)).toBeNull();
    expect(convertRequest(png, 'webp', false)).toEqual({ format: 'webp', keepBoth: false });
    expect(convertRequest(png, 'png', true)).toEqual({ format: 'png', keepBoth: true });
  });
});
