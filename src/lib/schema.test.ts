import { describe, expect, it } from 'vitest';
import { t } from './i18n';
import { panes, settingsFor } from './schema';
import { getPath } from './settings';
import { sample } from './settings.fixture';

describe('the list of settings', () => {
  const all = settingsFor(true);

  it('has unique ids', () => {
    const ids = all.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('points every value setting at a field that exists', () => {
    for (const s of all) {
      if (s.path) expect(getPath(sample, s.path), s.id).not.toBeUndefined();
    }
  });

  it('only depends on switches that exist', () => {
    for (const s of all) {
      if (!s.depends_on) continue;
      const parent = all.find((p) => p.path === s.depends_on);
      expect(parent?.control.kind, `${s.id} depends on ${s.depends_on}`).toBe('switch');
    }
  });

  it('has real strings for every label and help text', () => {
    for (const s of all) {
      expect(t(s.label), s.id).not.toBe('');
      if (s.help) expect(t(s.help), s.id).not.toBe('');
    }
    for (const p of panes) expect(t(p.label)).not.toBe('');
  });

  it('hides the macOS-only settings on Windows', () => {
    expect(settingsFor(false).some((s) => s.mac_only)).toBe(false);
    expect(all.some((s) => s.mac_only)).toBe(true);
  });

  it('puts every setting in a pane that exists', () => {
    const ids = panes.map((p) => p.id);
    for (const s of all) expect(ids).toContain(s.pane);
  });

  it('has the File handling pane with both output rules, the hover toggle and crop sizes', () => {
    const at = (id: string) => all.find((s) => s.id === id);
    expect(all.filter((s) => s.pane === 'files').map((s) => s.path)).toEqual([
      'images.output.optimised',
      'images.output.converted',
    ]);
    expect(at('hover_hotkeys')?.path).toBe('floating.hover_hotkeys');
    expect(at('crop_sizes')?.control.kind).toBe('sizes');
  });

  it('has the Hotkeys pane with every binding in order', () => {
    expect(all.filter((s) => s.pane === 'hotkeys').map((s) => s.path)).toEqual([
      'hotkeys.optimise',
      'hotkeys.aggressive',
      'hotkeys.cycle_state',
      'svc.shortcuts.preview',
      'svc.shortcuts.compare',
      'svc.shortcuts.step_down',
      undefined,
    ]);
    expect(panes.map((p) => p.id).slice(-2)).toEqual(['hotkeys', 'about']);
  });
});
