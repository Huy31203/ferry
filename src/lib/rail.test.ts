import { describe, expect, it } from 'vitest';
import { card } from './card.fixture';
import { needsOriginal, railActions } from './cards';
import {
  addable,
  DEFAULT_RAIL_TOOLS,
  dropIndex,
  editorRows,
  knownCount,
  moveItem,
  RAIL_TOOLS,
  toolLabel,
} from './rail';

const done = card({ kind: 'done' }, { can_restore: true });

describe('the rail catalog', () => {
  it("follows the user's order", () => {
    expect(railActions(done, false, ['copy', 'crop', 'downscale'])).toEqual([
      'copy',
      'crop',
      'downscale',
      'more',
    ]);
  });

  it('skips unknown ids without dropping them', () => {
    expect(railActions(done, false, ['trim', 'copy', 'pipeline:x'])).toEqual(['copy', 'more']);
    expect(editorRows(['trim', 'copy'])).toEqual([
      { id: 'trim', known: false },
      { id: 'copy', known: true },
    ]);
    expect(knownCount(['trim', 'copy'])).toBe(1);
  });

  it('shows at most six tools, compact at most four compact tools', () => {
    const every = RAIL_TOOLS.map((tool) => tool.id);
    expect(every).toHaveLength(11);
    expect(railActions(done, false, every)).toHaveLength(7);
    expect(railActions(done, true, every)).toEqual(['downscale', 'crop', 'restore', 'copy', 'more']);
  });

  it('shows restore only on a restorable done card even when chosen', () => {
    expect(railActions(done, false, ['restore'])).toEqual(['restore', 'more']);
    expect(railActions(card({ kind: 'done' }), false, ['restore'])).toEqual(['more']);
    expect(
      railActions(card({ kind: 'restored' }, { can_restore: true }), false, ['restore']),
    ).toEqual(['more']);
  });

  it('needs the original where the tool re-encodes', () => {
    expect(needsOriginal('aggressive')).toBe(true);
    expect(needsOriginal('fit')).toBe(true);
    for (const id of ['reveal', 'preview', 'rename']) expect(needsOriginal(id)).toBe(false);
  });

  it('lists what is not on the rail', () => {
    expect(addable(DEFAULT_RAIL_TOOLS).map((tool) => tool.id)).toEqual([
      'reveal',
      'aggressive',
      'rename',
      'preview',
      'fit',
    ]);
  });

  it('moves an item and finds where a drag lands', () => {
    expect(moveItem(['a', 'b', 'c'], 0, 2)).toEqual(['b', 'c', 'a']);
    expect(dropIndex(5, [10, 30, 50])).toBe(0);
    expect(dropIndex(35, [10, 30, 50])).toBe(2);
    expect(dropIndex(99, [10, 30, 50])).toBe(3);
  });

  it('matches the default rail in Rust', () => {
    expect(DEFAULT_RAIL_TOOLS).toEqual(['downscale', 'compression', 'crop', 'restore', 'compare', 'copy']);
  });

  it('labels tools, unknown ones by id, and Reveal per system', () => {
    expect(toolLabel('crop', true)).toBe('Crop');
    expect(toolLabel('reveal', true)).toBe('Show in Finder');
    expect(toolLabel('reveal', false)).toBe('Show in Explorer');
    expect(toolLabel('trim', true)).toBe('Not in this version of Ferry (trim)');
  });
});
