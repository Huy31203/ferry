import { describe, expect, it } from 'vitest';
import { clampPan, cssScale, dividerAt, nowBox, shotLabel, versioned, type Shot } from './view';

const original: Shot = { path: '/b/shot.png', bytes: 2_400_000, dims: [2880, 1800], format: 'PNG', version: 0 };
const now: Shot = { path: '/w/shot.webp', bytes: 380_000, dims: [1440, 900], format: 'WebP', version: 1_700_000 };

describe('shotLabel', () => {
  it('reads size, dimensions and format, and the change for the result', () => {
    expect(shotLabel(original)).toBe('2.4 MB · 2880×1800 · PNG');
    expect(shotLabel(now, original.bytes)).toBe('380 KB · 1440×900 · WebP (−84%)');
  });

  it('leaves out what it does not know', () => {
    expect(shotLabel({ ...now, dims: null, format: '' })).toBe('380 KB');
  });
});

describe('nowBox', () => {
  it('stretches a downscaled result to the original size so pixels line up', () => {
    expect(nowBox([2880, 1800], [1440, 900])).toEqual([2880, 1800]);
    expect(nowBox([2880, 1800], [1441, 900])).toEqual([2880, 1800]);
  });

  it('keeps a cropped result at its own size', () => {
    expect(nowBox([2880, 1800], [1600, 900])).toEqual([1600, 900]);
    expect(nowBox([2880, 1800], [1800, 1800])).toEqual([1800, 1800]);
  });

  it('survives a zero side', () => {
    expect(nowBox([0, 0], [0, 0])).toEqual([0, 0]);
  });
});

describe('cssScale', () => {
  it('shows one image pixel per screen pixel at 100 % and two at 200 %', () => {
    expect(cssScale(1, [2880, 1800], [1000, 700], 2)).toBe(0.5);
    expect(cssScale(2, [2880, 1800], [1000, 700], 2)).toBe(1);
    expect(cssScale(1, [800, 600], [1000, 700], 1)).toBe(1);
  });

  it('fits a large image into the stage', () => {
    expect(cssScale('fit', [2000, 1000], [1000, 700], 1)).toBe(0.5);
    expect(cssScale('fit', [1000, 2000], [1000, 700], 1)).toBe(0.35);
  });

  it('never enlarges a small image past 100 % to fit', () => {
    expect(cssScale('fit', [100, 50], [1000, 700], 1)).toBe(1);
    expect(cssScale('fit', [100, 50], [1000, 700], 2)).toBe(0.5);
  });

  it('treats a missing pixel ratio as 1', () => {
    expect(cssScale(1, [100, 50], [1000, 700], 0)).toBe(1);
  });
});

describe('clampPan', () => {
  it('lets a zoomed image pan as far as it overhangs, no further', () => {
    expect(clampPan([500, -500], [1600, 1000], [1000, 700])).toEqual([300, -150]);
    expect(clampPan([-20, 10], [1600, 1000], [1000, 700])).toEqual([-20, 10]);
  });

  it('keeps an image that fits centred', () => {
    expect(clampPan([40, 40], [800, 600], [1000, 700])).toEqual([0, 0]);
  });
});

describe('dividerAt', () => {
  it('follows the pointer and stops at the edges', () => {
    expect(dividerAt(150, 100, 200)).toBe(0.25);
    expect(dividerAt(0, 100, 200)).toBe(0);
    expect(dividerAt(900, 100, 200)).toBe(1);
  });

  it('stays centred on a stage with no width', () => {
    expect(dividerAt(10, 0, 0)).toBe(0.5);
  });
});

describe('versioned', () => {
  it('adds the file version as a query the asset protocol ignores', () => {
    expect(versioned('http://asset.localhost/%2Fw%2Fshot.webp', now)).toBe(
      'http://asset.localhost/%2Fw%2Fshot.webp?v=1700000',
    );
  });
});
