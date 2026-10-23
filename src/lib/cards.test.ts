import { describe, expect, it } from 'vitest';
import {
  autoHides,
  canDrag,
  dragAllFiles,
  hoverTarget,
  isBusy,
  needsOriginal,
  openTool,
  railActions,
  remove,
  upsert,
  type RailAction,
} from './cards';
import { card } from './card.fixture';

describe('card states', () => {
  it('treats queued and optimising as busy', () => {
    expect(isBusy({ kind: 'queued' })).toBe(true);
    expect(isBusy({ kind: 'optimising', progress: 0, step: 'x' })).toBe(true);
    expect(isBusy({ kind: 'done' })).toBe(false);
  });

  it('auto-hides finished cards but never busy or error cards', () => {
    expect(autoHides({ kind: 'done' })).toBe(true);
    expect(autoHides({ kind: 'already_optimal' })).toBe(true);
    expect(autoHides({ kind: 'larger' })).toBe(true);
    expect(autoHides({ kind: 'restored' })).toBe(true);
    expect(autoHides({ kind: 'error', reason: 'x' })).toBe(false);
    expect(autoHides({ kind: 'queued' })).toBe(false);
  });
});

describe('railActions', () => {
  it('lays out rail A, with restore only on a restorable done card', () => {
    expect(railActions(card({ kind: 'done' }, { can_restore: true }))).toEqual([
      'downscale',
      'compression',
      'crop',
      'restore',
      'compare',
      'copy',
      'more',
    ]);
    expect(railActions(card({ kind: 'larger' }))).toEqual([
      'downscale',
      'compression',
      'crop',
      'compare',
      'copy',
      'more',
    ]);
  });

  it('leaves Compression and Compare off a compact card', () => {
    expect(railActions(card({ kind: 'done' }, { can_restore: true }), true)).toEqual([
      'downscale',
      'crop',
      'restore',
      'copy',
      'more',
    ]);
  });

  it('keeps the tools on a restored card, which offers re-optimise instead of restore', () => {
    expect(railActions(card({ kind: 'restored' }, { can_restore: true }))).not.toContain('restore');
    expect(railActions(card({ kind: 'restored' }))).toContain('downscale');
  });

  it('has no rail while busy, on errors, or without an output file', () => {
    expect(railActions(card({ kind: 'queued' }))).toEqual([]);
    expect(railActions(card({ kind: 'error', reason: 'x' }))).toEqual([]);
    expect(railActions(card({ kind: 'done' }, { output: null }))).toEqual([]);
  });

  it('turns off only the tools that need the original', () => {
    expect(['downscale', 'compression', 'crop', 'compare'].every((a) => needsOriginal(a as RailAction))).toBe(true);
    expect(['restore', 'copy', 'more'].some((a) => needsOriginal(a as RailAction))).toBe(false);
  });
});

describe('the stack', () => {
  it('updates a card in place and adds new ones at the end', () => {
    const first = card({ kind: 'queued' });
    const second = card({ kind: 'queued' }, { id: 'b' });
    let cards = upsert(upsert([], first), second);
    cards = upsert(cards, card({ kind: 'done' }));
    expect(cards.map((c) => [c.id, c.state.kind])).toEqual([
      ['a', 'done'],
      ['b', 'queued'],
    ]);
  });

  it('drops the card the app closed and keeps the rest in order', () => {
    const cards = [card({ kind: 'done' }, { id: '1' }), card({ kind: 'done' }, { id: '2' })];
    expect(remove(cards, '1').map((c) => c.id)).toEqual(['2']);
    expect(remove(cards, 'gone')).toEqual(cards);
  });

  it('drags every finished output and nothing else', () => {
    const cards = [
      card({ kind: 'done' }, { id: '1', output: '/w/1.png' }),
      card({ kind: 'optimising', progress: 0, step: '' }, { id: '2', output: '/w/2.png' }),
      card({ kind: 'error', reason: 'x' }, { id: '3', output: '/w/3.png' }),
      card({ kind: 'already_optimal' }, { id: '4', output: '/w/4.png' }),
    ];
    expect(dragAllFiles(cards)).toEqual(['/w/1.png', '/w/4.png']);
    expect(canDrag(cards[1])).toBe(false);
  });
});

describe('the open tool', () => {
  const tool = { id: 'a', kind: 'rename' as const };

  it('stays open while its card is finished and idle', () => {
    expect(openTool(tool, [card({ kind: 'done' })])).toEqual(tool);
    expect(openTool(null, [card({ kind: 'done' })])).toBeNull();
  });

  it('closes when its card is gone, busy, or has no file', () => {
    expect(openTool(tool, [])).toBeNull();
    expect(openTool(tool, [card({ kind: 'queued' })])).toBeNull();
    expect(openTool(tool, [card({ kind: 'done' }, { output: null })])).toBeNull();
    expect(openTool(tool, [card({ kind: 'done' }, { id: 'b' })])).toBeNull();
  });
});

describe('the hover hotkeys target', () => {
  const cards = [card({ kind: 'done' })];

  it('is the idle card under the pointer', () => {
    expect(hoverTarget('a', cards, null, true)).toBe('a');
    expect(hoverTarget(null, cards, null, true)).toBeNull();
  });

  it('is nothing while a tool has the keyboard or the setting is off', () => {
    expect(hoverTarget('a', cards, { id: 'a', kind: 'rename' }, true)).toBeNull();
    expect(hoverTarget('a', cards, { id: 'b', kind: 'crop' }, true)).toBeNull();
    expect(hoverTarget('a', cards, null, false)).toBeNull();
  });

  it('is nothing once the card is gone, busy or has no file', () => {
    expect(hoverTarget('a', [], null, true)).toBeNull();
    expect(hoverTarget('a', [card({ kind: 'queued' })], null, true)).toBeNull();
    expect(hoverTarget('a', [card({ kind: 'done' }, { output: null })], null, true)).toBeNull();
  });
});
