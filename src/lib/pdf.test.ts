import { describe, expect, it } from 'vitest';
import { card } from './card.fixture';
import { checkPages, orientRatio, pdfCropForm, pdfCropPatch, pdfFrame } from './pdf';

describe('pdf crop', () => {
  it('turns a ratio to the page', () => {
    expect(orientRatio([842, 595], [9, 16])).toEqual([16, 9]);
    expect(orientRatio([595, 842], [16, 9])).toEqual([9, 16]);
    expect(orientRatio([500, 500], [4, 3])).toEqual([4, 3]);
  });
  it('frames a centred crop on the poster', () => {
    const f = pdfFrame([595, 842], [1640, 2360])!;
    expect(f.y).toBe(0);
    expect(f.x).toBeCloseTo((1 - (842 * 1640) / 2360 / 595) / 2, 3);
    expect(pdfFrame([595, 842], null)).toBeNull();
    expect(pdfFrame(null, [4, 3])).toBeNull();
  });
  it('sends the ratio and the uncrop switch', () => {
    expect(pdfCropPatch({ aspect: null, uncrop: true })).toEqual({ aspect: null, uncrop: true });
  });
  it('starts from the card', () => {
    const edits = { ...card({ kind: 'done' }).edits, aspect: [4, 3] as [number, number], uncrop: true };
    expect(pdfCropForm(edits)).toEqual({ aspect: [4, 3], uncrop: true });
  });
});

describe('pdf pages', () => {
  it('checks a page range against the page count', () => {
    const form = { format: 'png' as const, all: false, from: '2', to: '3', dpi: 150 };
    expect(checkPages(form, 3)).toEqual({ patch: { format: 'png', pages: { first: 2, last: 3 }, dpi: 150 }, error: null });
    expect(checkPages({ ...form, to: '4' }, 3).error).toBe('Choose pages from 1 to 3');
    expect(checkPages({ ...form, from: 'x' }, 3).patch).toBeNull();
    expect(checkPages({ ...form, all: true, from: 'x' }, 3).patch).toEqual({ format: 'png', pages: null, dpi: 150 });
  });
});
