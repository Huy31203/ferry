import { describe, expect, it } from 'vitest';
import { card } from './card.fixture';
import type { JobUpdate } from './ipc';
import {
  NO_SELECTION,
  clearSel,
  escapeSel,
  headerText,
  keyPatch,
  listKey,
  listTotals,
  moveCursor,
  nextListMode,
  prune,
  rowsHeld,
  selectAll,
  stepDown,
  targets,
  toggle,
  type KeyPress,
} from './selection';

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

  const abcd = ['a', 'b', 'c', 'd'].map((id) => card({ kind: 'done' }, { id }));

  it('toggles, selects all and clears but keeps the cursor', () => {
    const one = toggle(NO_SELECTION, 'b');
    expect(one).toEqual({ ids: ['b'], anchor: 'b', cursor: 'b' });
    expect(toggle(one, 'b').ids).toEqual([]);
    expect(selectAll(one, abcd).ids).toEqual(['a', 'b', 'c', 'd']);
    expect(clearSel(selectAll(one, abcd))).toEqual({ ids: [], anchor: null, cursor: 'b' });
  });

  it('moves the cursor and extends from the anchor in card order', () => {
    expect(moveCursor(NO_SELECTION, abcd, 1, false).cursor).toBe('d');
    expect(moveCursor(NO_SELECTION, abcd, -1, false).cursor).toBe('a');
    let sel = toggle(NO_SELECTION, 'b');
    sel = moveCursor(sel, abcd, 1, true);
    sel = moveCursor(sel, abcd, 1, true);
    expect(sel.ids).toEqual(['b', 'c', 'd']);
    expect(moveCursor(sel, abcd, 1, false).cursor).toBe('d');
  });

  it('forgets cards that are gone', () => {
    const sel = { ids: ['a', 'z'], anchor: 'z', cursor: 'a' };
    expect(prune(sel, abcd)).toEqual({ ids: ['a'], anchor: null, cursor: 'a' });
  });

  it('splits a selection into what can be dragged and what can be edited', () => {
    const cards = [
      card({ kind: 'done' }, { id: 'a' }),
      card({ kind: 'optimising', progress: 0.5, step: 's' }, { id: 'b' }),
      card({ kind: 'done' }, { id: 'c', can_edit: false }),
    ];
    const got = targets(cards, ['a', 'b', 'c']);
    expect(got.fileIds).toEqual(['a', 'c']);
    expect(got.edit).toEqual(['a', 'b']);
    expect(got.cantEdit).toBe(1);
  });

  it('reads list keys with the platform command key and leaves every other key alone', () => {
    const k = (key: string, mods: Partial<KeyPress> = {}): KeyPress => ({
      key,
      shiftKey: false,
      altKey: false,
      metaKey: false,
      ctrlKey: false,
      ...mods,
    });
    expect(listKey(k('a', { metaKey: true }), true)).toEqual({ kind: 'select_all' });
    expect(listKey(k('a', { ctrlKey: true }), true)).toBeNull();
    expect(listKey(k('a', { ctrlKey: true }), false)).toEqual({ kind: 'select_all' });
    expect(listKey(k('d', { metaKey: true }), true)).toEqual({ kind: 'compare' });
    expect(listKey(k('Escape'), true)).toEqual({ kind: 'escape' });
    expect(listKey(k('ArrowDown'), true)).toEqual({ kind: 'move', delta: 1, extend: false });
    expect(listKey(k('ArrowUp', { shiftKey: true }), false)).toEqual({ kind: 'move', delta: -1, extend: true });
    expect(listKey(k(' '), true)).toEqual({ kind: 'preview' });
    expect(listKey(k('-'), true)).toEqual({ kind: 'step_down' });
    expect(listKey(k('5'), true)).toEqual({ kind: 'scale', percent: 50 });
    for (const other of [k('0'), k('Enter'), k('a'), k('ArrowDown', { altKey: true }), k(' ', { shiftKey: true })]) {
      expect(listKey(other, true)).toBeNull();
    }
  });

  it('clears the selection on Esc and hands the keyboard back when nothing is selected', () => {
    const two = { ids: ['a', 'b'], anchor: 'a', cursor: 'b' };
    expect(escapeSel(two)).toEqual({ sel: { ids: [], anchor: null, cursor: 'b' }, release: false });
    expect(escapeSel(NO_SELECTION).release).toBe(true);
  });

  it('steps down like the hover key', () => {
    expect([stepDown(null), stepDown(50), stepDown(15), stepDown(10)]).toEqual([90, 40, 10, null]);
  });
});

describe('keyPatch', () => {
  const cfg = { dpi: 150, dpi_aggressive: 100, extract_dpi: 150, devices: [] };
  const pdf = (edits: Partial<JobUpdate['edits']> = {}) => {
    const c = card({ kind: 'done' }, { source_format: 'pdf' });
    return { ...c, edits: { ...c.edits, ...edits } };
  };

  it('scales an image by digits and steps it down with -', () => {
    const c = card({ kind: 'done' });
    expect(keyPatch({ kind: 'scale', percent: 50 }, c, cfg)).toEqual({ scale: 50 });
    expect(keyPatch({ kind: 'step_down' }, c, cfg)).toEqual({ scale: 90 });
    expect(keyPatch({ kind: 'step_down' }, { ...c, edits: { ...c.edits, scale: 10 } }, cfg)).toBeNull();
  });

  it('does nothing on a PDF for digits, and steps its DPI with -', () => {
    expect(keyPatch({ kind: 'scale', percent: 50 }, pdf(), cfg)).toBeNull();
    expect(keyPatch({ kind: 'step_down' }, pdf(), cfg)).toEqual({ dpi: 100 });
    expect(keyPatch({ kind: 'step_down' }, pdf({ dpi: 200 }), cfg)).toEqual({ dpi: 150 });
    expect(keyPatch({ kind: 'step_down' }, pdf({ level: 'aggressive' }), cfg)).toEqual({ dpi: 72 });
    expect(keyPatch({ kind: 'step_down' }, pdf({ dpi: 48 }), cfg)).toBeNull();
  });
});
