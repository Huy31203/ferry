import { describe, expect, it } from 'vitest';
import { card } from './card.fixture';
import { formatChoices } from './edits';
import type { VideoMeta } from './ipc';
import {
  RING,
  SPEED_CHIPS,
  extLabel,
  formatDuration,
  isVideo,
  parseSpeed,
  ringDash,
  speedLabel,
  speedPatch,
} from './video';

const meta: VideoMeta = {
  duration_ms: 1000,
  codec: 'HEVC',
  fps: 30,
  audio: true,
  original: '/b/a.mp4',
  original_duration_ms: 1000,
  hevc_encoder: true,
};

describe('video helpers', () => {
  it('knows video formats', () => {
    expect(isVideo('mkv') && isVideo('hevc')).toBe(true);
    expect(isVideo('png') || isVideo('gif') || isVideo(null)).toBe(false);
  });

  it('formats durations', () => {
    expect(formatDuration(0)).toBe('0:00');
    expect(formatDuration(5400)).toBe('0:05');
    expect(formatDuration(62000)).toBe('1:02');
    expect(formatDuration(3723000)).toBe('1:02:03');
  });

  it('labels speeds', () => {
    expect(SPEED_CHIPS).toEqual([125, 150, 200, 300]);
    expect(speedLabel(125)).toBe('1.25×');
    expect(speedLabel(150)).toBe('1.5×');
    expect(speedLabel(200)).toBe('2×');
  });

  it('reads a typed speed', () => {
    expect(parseSpeed('1.5')).toBe(150);
    expect(parseSpeed('1,5')).toBe(150);
    expect(parseSpeed('2x')).toBe(200);
    expect(parseSpeed('2×')).toBe(200);
    expect(parseSpeed('0.5')).toBe(50);
    expect(parseSpeed('4')).toBe(400);
    for (const bad of ['0.4', '5', '', 'abc']) expect(parseSpeed(bad)).toBeNull();
  });

  it('builds the speed patch', () => {
    expect(speedPatch(null, true)).toEqual({ speed: null });
    expect(speedPatch(100, false)).toEqual({ speed: null });
    expect(speedPatch(150, false)).toEqual({ speed: { percent: 150, keep_frames: false } });
  });

  it('draws the ring', () => {
    expect(RING).toBeCloseTo(150.8, 1);
    expect(ringDash(0.25)).toBe('37.7 150.8');
  });

  it('offers video targets and GIF on a video card', () => {
    const c = card({ kind: 'done' }, { format: 'hevc', source_format: 'mp4', ext: 'mp4', video: meta });
    expect(formatChoices(c).map((x) => [x.format, x.current, x.firstFrame])).toEqual([
      ['mp4', false, false],
      ['hevc', true, false],
      ['av1', false, false],
      ['webm', false, false],
      ['gif', false, false],
    ]);
  });

  it('names the codec beside the extension', () => {
    expect(extLabel(card({ kind: 'done' }, { ext: 'mp4', video: meta }))).toBe('mp4 · HEVC');
    expect(extLabel(card({ kind: 'done' }))).toBe('png');
  });

  it('hides HEVC without a hardware encoder', () => {
    const noHevc = { ...meta, codec: 'H.264', hevc_encoder: false };
    const c = card({ kind: 'done' }, { format: 'mp4', source_format: 'mp4', ext: 'mp4', video: noHevc });
    expect(formatChoices(c).map((x) => x.format)).toEqual(['mp4', 'av1', 'webm', 'gif']);
    expect(formatChoices({ ...c, video: meta }).map((x) => x.format)).toEqual([
      'mp4',
      'hevc',
      'av1',
      'webm',
      'gif',
    ]);
  });
});
