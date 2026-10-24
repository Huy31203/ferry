import { describe, expect, it } from 'vitest';
import { card } from './card.fixture';
import { headerText, listTotals, nextListMode, rowsHeld } from './selection';

describe('compact list', () => {
  it('switches to the list above the threshold but never while held', () => {
    expect(nextListMode(6, 5, false, false)).toBe(true);
    expect(nextListMode(5, 5, true, false)).toBe(false);
    expect(nextListMode(9, 5, false, true)).toBe(false);
    expect(nextListMode(2, 5, true, true)).toBe(true);
    expect(nextListMode(0, 5, true, true)).toBe(false);
  });

  it('holds rows while pointed at, selected, edited or dragged', () => {
    expect(rowsHeld(false, 0, false, false)).toBe(false);
    expect(rowsHeld(true, 0, false, false)).toBe(true);
    expect(rowsHeld(false, 2, false, false)).toBe(true);
    expect(rowsHeld(false, 0, true, false)).toBe(true);
    expect(rowsHeld(false, 0, false, true)).toBe(true);
  });

  it('totals what was saved like the spec', () => {
    const done = Array.from({ length: 20 }, (_, i) =>
      card({ kind: 'done' }, { id: `d${i}`, before: 3_030_000, after: 970_000 }),
    );
    const queued = Array.from({ length: 3 }, (_, i) => card({ kind: 'queued' }, { id: `q${i}` }));
    expect(headerText(listTotals([...done, ...queued]), 'en-US')).toBe(
      '23 files · 41.2 MB saved (−68%)',
    );
  });

  it('shows only the count when nothing was saved', () => {
    const queued = Array.from({ length: 6 }, (_, i) => card({ kind: 'queued' }, { id: `q${i}` }));
    expect(headerText(listTotals(queued), 'en-US')).toBe('6 files');
  });
});
