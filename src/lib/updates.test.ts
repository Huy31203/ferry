import { describe, expect, it } from 'vitest';
import { formatBytes } from './format';
import type { UpdateState, UpdateView } from './svc';
import { updateText } from './updates';

function view(state: UpdateState, extra: Partial<UpdateView> = {}): UpdateView {
  return { current: '0.2.0', state, skipped: false, pill: false, release_url: null, ...extra };
}

describe('updateText', () => {
  it('offers a first check while idle', () => {
    const text = updateText(view({ kind: 'idle' }));
    expect([text.line, text.buttons, text.busy]).toEqual(['svc.updates.idle', ['check'], false]);
  });

  it('shows no button while checking', () => {
    const text = updateText(view({ kind: 'checking' }));
    expect([text.line, text.buttons, text.busy]).toEqual(['svc.updates.checking', [], true]);
  });

  it('says up to date and offers another check', () => {
    const text = updateText(view({ kind: 'up_to_date' }));
    expect([text.line, text.buttons]).toEqual(['svc.updates.up_to_date', ['check']]);
  });

  it('offers an available version with its notes', () => {
    const text = updateText(view({ kind: 'available', version: '0.3.0', notes: 'Fixes\nMore' }));
    expect(text.line).toBe('svc.updates.available');
    expect(text.vars).toEqual({ version: '0.3.0' });
    expect(text.buttons).toEqual(['install', 'later', 'skip', 'release']);
    expect(text.notes).toBe('Fixes\nMore');
  });

  it('says so when a release has no notes', () => {
    const text = updateText(view({ kind: 'available', version: '0.3.0', notes: '' }));
    expect(text.notes).toBe('This release has no notes.');
  });

  it('keeps only install and the release link for a skipped version', () => {
    const text = updateText(
      view({ kind: 'available', version: '0.3.0', notes: 'x' }, { skipped: true }),
    );
    expect(text.line).toBe('svc.updates.available');
    expect(text.buttons).toEqual(['install', 'release']);
  });

  it('shows a percent while the size is known', () => {
    const text = updateText(
      view({ kind: 'downloading', version: '0.3.0', received: 42, total: 100 }),
    );
    expect(text.line).toBe('svc.updates.downloading');
    expect(text.vars).toEqual({ version: '0.3.0', percent: '42 %' });
    expect([text.buttons, text.busy]).toEqual([[], true]);
  });

  it('shows the bytes received when the size is unknown', () => {
    const text = updateText(
      view({ kind: 'downloading', version: '0.3.0', received: 1_500_000, total: null }),
    );
    expect(text.line).toBe('svc.updates.downloading_bytes');
    expect(text.vars).toEqual({ version: '0.3.0', size: formatBytes(1_500_000) });
  });

  it('waits as ready, with install and later', () => {
    const text = updateText(view({ kind: 'ready', version: '0.3.0', busy: 2 }));
    expect(text.line).toBe('svc.updates.ready');
    expect(text.vars).toEqual({ version: '0.3.0' });
    expect(text.buttons).toEqual(['install', 'later']);
  });

  it('shows no button while installing', () => {
    const text = updateText(view({ kind: 'installing', version: '0.3.0' }));
    expect([text.line, text.buttons, text.busy]).toEqual(['svc.updates.installing', [], true]);
  });

  it('names the phase of a failure and offers to try again', () => {
    for (const phase of ['check', 'download', 'install'] as const) {
      const text = updateText(view({ kind: 'failed', phase, reason: 'offline' }));
      expect(text.line).toBe(`svc.updates.failed.${phase}`);
      expect(text.vars).toEqual({ why: 'offline' });
      expect(text.buttons).toEqual(['retry']);
    }
  });

  it('never offers a button while busy', () => {
    const states: UpdateState[] = [
      { kind: 'checking' },
      { kind: 'downloading', version: '0.3.0', received: 1, total: 2 },
      { kind: 'installing', version: '0.3.0' },
    ];
    for (const state of states) {
      const text = updateText(view(state));
      expect(text.busy).toBe(true);
      expect(text.buttons).toEqual([]);
    }
  });
});
