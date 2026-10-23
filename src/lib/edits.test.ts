import { describe, expect, it } from 'vitest';
import { card } from './card.fixture';
import {
  QUALITY_STOPS,
  SCALE_STOPS,
  addSize,
  parseSize,
  sizeLabel,
  checkCrop,
  labelledStops,
  sliderTrack,
  convertRequest,
  cropForm,
  cropFrame,
  currentQuality,
  fitBytes,
  fitForm,
  fitRatio,
  formatChoices,
  frameBox,
  nextQuality,
  nextScale,
  qualityLabel,
  qualityPatch,
  scaleForm,
  scaleLabel,
  scalePatch,
  sliderIndex,
  stopIndex,
  type CropForm,
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

describe('slider labels', () => {
  it('sizes the track so the panel (46 px down, 59 px of chrome) ends 6 px above the card bottom', () => {
    expect(sliderTrack(200)).toBe(89);
    expect(sliderTrack(150)).toBe(39);
  });

  it('labels the first, the last and the multiples of 25 where they are 14 px or more apart', () => {
    // 100, 90, 85 … 10: 75 is stop 4, 50 stop 9, 25 stop 14
    expect(labelledStops(SCALE_STOPS, 89)).toEqual([0, 4, 9, 14, 17]);
    // 100, 95 … 5, Extreme: 75 is stop 5, 50 stop 10, 25 stop 15
    expect(labelledStops(QUALITY_STOPS, 89)).toEqual([0, 5, 10, 15, 20]);
  });

  it('drops the round stop nearest an end when two labels are too close, on a compact card', () => {
    expect(labelledStops(SCALE_STOPS, 39)).toEqual([0, 9, 17]);
    expect(labelledStops(QUALITY_STOPS, 39)).toEqual([0, 10, 20]);
  });

  it('never labels two stops closer than 14 px', () => {
    for (const stops of [SCALE_STOPS, QUALITY_STOPS]) {
      for (const track of [sliderTrack(200), sliderTrack(150)]) {
        const at = labelledStops(stops, track).map((i) => (i / (stops.length - 1)) * track);
        at.slice(1).forEach((y, n) => expect(y - at[n]).toBeGreaterThanOrEqual(14));
      }
    }
  });

  it('labels a lone stop', () => {
    expect(labelledStops([100], 89)).toEqual([0]);
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

describe('crop popover', () => {
  const form = (extra: Partial<CropForm> = {}): CropForm => ({ ...cropForm(edits), ...extra });
  const dims: [number, number] = [2880, 1800];

  it('starts from the card’s current crop', () => {
    expect(cropForm(edits)).toEqual({
      aspect: null,
      mode: 'size',
      width: '0',
      height: '0',
      edge: '0',
      framing: 'centre',
    });
    const cropped = { ...edits, aspect: [16, 9] as [number, number], size: { kind: 'long_edge' as const, edge: 1200 }, framing: 'smart' as const };
    expect(cropForm(cropped)).toMatchObject({ aspect: [16, 9], mode: 'long_edge', edge: '1200', framing: 'smart' });
  });

  it('turns the fields into a size: 0 keeps the ratio', () => {
    expect(checkCrop(form({ width: '1200', height: '628' }), dims).patch).toEqual({
      aspect: null,
      framing: 'centre',
      size: { kind: 'exact', width: 1200, height: 628 },
    });
    expect(checkCrop(form({ width: '1440' }), dims).patch?.size).toEqual({ kind: 'width', width: 1440 });
    expect(checkCrop(form({ height: '900' }), dims).patch?.size).toEqual({ kind: 'height', height: 900 });
    expect(checkCrop(form({ mode: 'long_edge', edge: '1000' }), dims).patch?.size).toEqual({
      kind: 'long_edge',
      edge: 1000,
    });
  });

  it('crops to a ratio alone, with no size', () => {
    expect(checkCrop(form({ aspect: [16, 9] }), dims).patch).toEqual({
      aspect: [16, 9],
      framing: 'centre',
      size: null,
    });
  });

  it('refuses words, huge sides and an empty Free crop', () => {
    expect(checkCrop(form({ width: '12a' }), dims)).toMatchObject({ error: 'Whole numbers only', field: 'width' });
    expect(checkCrop(form({ height: '-5' }), dims)).toMatchObject({ error: 'Whole numbers only', field: 'height' });
    expect(checkCrop(form({ width: '30001' }), dims)).toMatchObject({ error: 'At most 30000 px', field: 'width' });
    expect(checkCrop(form(), dims)).toMatchObject({ error: 'Choose a ratio or a size', field: null });
  });

  it('hints when the size is larger than the original', () => {
    expect(checkCrop(form({ width: '4000' }), dims).larger).toBe(true);
    expect(checkCrop(form({ width: '1440' }), dims).larger).toBe(false);
    expect(checkCrop(form({ width: '4000' }), null).larger).toBe(false);
  });

  it('halves and doubles the sizes, within 1 to 30000', () => {
    expect(scaleForm(form({ width: '1440', height: '0' }), 0.5)).toMatchObject({ width: '720', height: '0' });
    expect(scaleForm(form({ width: '20000', height: '1' }), 2)).toMatchObject({ width: '30000', height: '2' });
    expect(scaleForm(form({ width: '1', edge: 'x' }), 0.5)).toMatchObject({ width: '1', edge: 'x' });
  });

  it('fits a ratio like ferry_core', () => {
    expect(fitRatio([2880, 1800], [16, 9])).toEqual([2880, 1620]);
    expect(fitRatio([2880, 1800], [1, 1])).toEqual([1800, 1800]);
    expect(fitRatio([1800, 2880], [16, 9])).toEqual([1800, 1013]);
    expect(fitRatio([3, 1], [1, 100])).toEqual([1, 1]);
  });

  it('frames the crop on the thumbnail', () => {
    expect(cropFrame(dims, form())).toBeNull();
    expect(cropFrame(null, form({ aspect: [1, 1] }))).toBeNull();
    const square = cropFrame(dims, form({ aspect: [1, 1] }));
    expect(square).toEqual({ x: 540 / 2880, y: 0, w: 1800 / 2880, h: 1 });
    // A 2880×1800 picture in a 300×200 card is 300×187.5, 6.25 px from the top.
    expect(frameBox(square!, dims, [300, 200])).toEqual({ left: 56.25, top: 6.25, width: 187.5, height: 187.5 });
  });
});

describe('fit popover', () => {
  it('reads KB and MB in decimal units', () => {
    expect(fitBytes('1', 'mb')).toBe(1_000_000);
    expect(fitBytes('1.5', 'mb')).toBe(1_500_000);
    expect(fitBytes('250', 'kb')).toBe(250_000);
  });

  it('refuses zero, negatives and words', () => {
    expect(fitBytes('0', 'mb')).toBeNull();
    expect(fitBytes('-1', 'mb')).toBeNull();
    expect(fitBytes('', 'kb')).toBeNull();
    expect(fitBytes('big', 'mb')).toBeNull();
  });

  it('starts from the card’s target', () => {
    expect(fitForm(null)).toEqual({ text: '', unit: 'mb' });
    expect(fitForm(5_000_000)).toEqual({ text: '5', unit: 'mb' });
    expect(fitForm(250_000)).toEqual({ text: '250', unit: 'kb' });
  });
});

describe('saved crop sizes', () => {
  it('reads width×height with x, × or spaces', () => {
    expect(parseSize('1200×628')).toEqual([1200, 628]);
    expect(parseSize(' 1080 x 1080 ')).toEqual([1080, 1080]);
    expect(parseSize('640X480')).toEqual([640, 480]);
  });

  it('refuses zero, huge sides and words', () => {
    expect(parseSize('0×100')).toBeNull();
    expect(parseSize('30001×10')).toBeNull();
    expect(parseSize('wide')).toBeNull();
    expect(parseSize('1200')).toBeNull();
  });

  it('puts a new size first and never twice', () => {
    const sizes: [number, number][] = [
      [1200, 628],
      [1080, 1080],
    ];
    expect(addSize(sizes, [640, 480])).toEqual([[640, 480], [1200, 628], [1080, 1080]]);
    expect(addSize(sizes, [1080, 1080])).toEqual([[1080, 1080], [1200, 628]]);
    expect(sizeLabel([1200, 628])).toBe('1200×628');
  });
});
