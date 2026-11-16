import { describe, expect, it } from 'vitest';
import { NO_DRAG, countLabel, stripHeight, stripState } from './strip';

const base = { count: 0, canUndo: false, hover: false, summoned: false, drag: NO_DRAG };

describe('stripState', () => {
  it('is hidden at rest, with or without items', () => {
    expect(stripState(base)).toBe('hidden');
    expect(stripState({ ...base, count: 3 })).toBe('hidden');
  });
  it('opens when summoned or hovered', () => {
    expect(stripState({ ...base, count: 3, summoned: true })).toBe('open');
    expect(stripState({ ...base, count: 3, hover: true })).toBe('open');
    expect(stripState({ ...base, summoned: true })).toBe('open');
  });
  it('shows the strip for an offered drag, even when empty, and becomes the target near it', () => {
    const offer = { offer: true, near: false, own: false };
    expect(stripState({ ...base, drag: offer })).toBe('resting');
    expect(stripState({ ...base, drag: { ...offer, near: true } })).toBe('target');
  });
  it('stays open with no target while its own pile is dragged', () => {
    const own = { offer: false, near: true, own: true };
    expect(stripState({ ...base, count: 2, drag: own })).toBe('open');
  });
  it('shows the cleared note while Undo is possible, and an add dismisses it', () => {
    expect(stripState({ ...base, canUndo: true })).toBe('cleared');
    expect(stripState({ ...base, count: 1, canUndo: false })).toBe('hidden');
  });
});

describe('stripHeight', () => {
  it('maps states to the panel height', () => {
    expect(stripHeight('hidden')).toBe(0);
    expect(stripHeight('resting')).toBe(27);
    expect(stripHeight('cleared')).toBe(27);
    expect(stripHeight('open')).toBe(170);
    expect(stripHeight('target')).toBe(170);
  });
});

describe('countLabel', () => {
  it('says Item or Items', () => {
    expect(countLabel(1)).toBe('1 Item');
    expect(countLabel(8)).toBe('8 Items');
  });
});
