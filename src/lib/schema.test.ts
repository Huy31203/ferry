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
});
