import { describe, expect, it } from 'vitest';
import { autoHides, canDrag, dragAllFiles, isBusy, railActions, upsert } from './cards';
import type { JobState, JobUpdate } from './ipc';

function card(state: JobState, extra: Partial<JobUpdate> = {}): JobUpdate {
  return {
    id: 'a',
    state,
    name: 'shot',
    ext: 'png',
    before: 2_000_000,
    after: null,
    detail: '',
    thumb_path: '/b/shot.png',
    source: 'clipboard',
    can_restore: false,
    can_retry: false,
    output: '/w/shot.png',
    ...extra,
  };
}

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
  it('offers restore only on a restorable done card', () => {
    expect(railActions(card({ kind: 'done' }, { can_restore: true }))).toEqual([
      'copy',
      'reveal',
      'restore',
      'aggressive',
    ]);
    expect(railActions(card({ kind: 'larger' }))).toEqual(['copy', 'reveal', 'aggressive']);
  });

  it('leaves aggressive off a restored card, which offers re-optimise instead', () => {
    expect(railActions(card({ kind: 'restored' }))).toEqual(['copy', 'reveal']);
  });

  it('has no rail while busy, on errors, or without an output file', () => {
    expect(railActions(card({ kind: 'queued' }))).toEqual([]);
    expect(railActions(card({ kind: 'error', reason: 'x' }))).toEqual([]);
    expect(railActions(card({ kind: 'done' }, { output: null }))).toEqual([]);
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
