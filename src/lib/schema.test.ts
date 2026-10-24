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

  it('has a Videos pane with the encoder cards first', () => {
    const videos = all.filter((s) => s.pane === 'videos');
    expect(videos[0].control.kind).toBe('encoder');
    expect(videos.map((s) => s.path)).toEqual([
      'video.encoder',
      'video.compression',
      'video.convert_mov',
      'video.audio_to_aac',
      'video.remove_audio',
      'video.fps_cap',
      'video.speed_frames',
      'video.preserve_dates',
      'video.ignored_extensions',
      'video.max_size_mb',
    ]);
  });

  it('has the File handling pane with both output rules, the hover toggle and crop sizes', () => {
    const at = (id: string) => all.find((s) => s.id === id);
    expect(all.filter((s) => s.pane === 'files').map((s) => s.path)).toEqual([
      'images.output.optimised',
      'images.output.converted',
      'video.output.optimised',
      'video.output.converted',
    ]);
    expect(at('hover_hotkeys')?.path).toBe('floating.hover_hotkeys');
    expect(at('crop_sizes')?.control.kind).toBe('sizes');
  });

  it('has the clipboard output rows, none tied to automatic clipboard work', () => {
    const rows = ['accumulate', 'copy_paths', 'auto_copy'].map((id) => all.find((s) => s.id === id));
    expect(rows.map((s) => s?.path)).toEqual(['clipboard.accumulate', 'clipboard.copy_paths', 'clip.auto_copy']);
    expect(rows.map((s) => s?.depends_on)).toEqual([undefined, undefined, undefined]);
    expect(t('clip.auto_copy')).toBe('Copy results of drops and watched folders');
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

  it('puts the rail editor and the update pill under Floating results', () => {
    const at = (id: string) => all.find((s) => s.id === id);
    expect([at('svc_rail')?.pane, at('svc_rail')?.path]).toEqual(['floating', 'svc.rail.tools']);
    expect([at('svc_update_pill')?.pane, at('svc_update_pill')?.path]).toEqual([
      'floating',
      'svc.updates.pill',
    ]);
  });

  it('has the About pane with the update panel and the automatic-check switch', () => {
    expect(all.filter((s) => s.pane === 'about').map((s) => s.id)).toEqual([
      'svc_update',
      'svc_auto_check',
    ]);
  });

  it('ends the General pane with the sync row under its own heading', () => {
    const general = all.filter((s) => s.pane === 'general');
    const last = general[general.length - 1];
    expect([last.id, last.group]).toEqual(['svc_sync', 'svc.group.sync']);
  });
});
