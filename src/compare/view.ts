import { formatBytes, savingPercent } from '../lib/format';

/** Mirrors compare::Shot (src-tauri/src/compare.rs). */
export interface Shot {
  path: string;
  bytes: number;
  /** As shown (JPEG orientation applied); null when unknown. */
  dims: [number, number] | null;
  format: string;
  /** Changes whenever the file is rewritten. */
  version: number;
}

/** Mirrors compare::CompareView. `original` is null in single-image mode (Windows quick preview). */
export interface CompareView {
  id: string;
  name: string;
  original: Shot | null;
  now: Shot;
}

export type Mode = 'slider' | 'sides';
export type Zoom = 'fit' | 1 | 2;
export type Size = [number, number];

/** "2.4 MB · 2880×1800 · PNG"; the result adds its change: "380 KB · 1440×900 · WebP (−84%)". */
export function shotLabel(shot: Shot, before?: number): string {
  const parts = [formatBytes(shot.bytes)];
  if (shot.dims) parts.push(`${shot.dims[0]}×${shot.dims[1]}`);
  if (shot.format) parts.push(shot.format);
  const text = parts.join(' · ');
  return before === undefined ? text : `${text} (${savingPercent(before, shot.bytes)})`;
}

/**
 * The size, in image pixels, the result is drawn at. A downscaled result has
 * the original's shape and is stretched to the original's size so pixels line
 * up; a cropped one has another shape and keeps its own size, centred.
 */
export function nowBox(original: Size, now: Size): Size {
  const shape = (size: Size) => size[0] / Math.max(1, size[1]);
  return Math.abs(shape(original) / shape(now) - 1) < 0.01 ? original : now;
}

/**
 * CSS pixels per image pixel. 100 % is one image pixel per screen pixel, so a
 * Retina screenshot shows at its real sharpness. Fit shrinks the image into the
 * stage but never enlarges it past 100 %.
 */
export function cssScale(zoom: Zoom, box: Size, stage: Size, dpr: number): number {
  const actual = 1 / Math.max(1, dpr);
  if (zoom !== 'fit') return actual * zoom;
  return Math.min(actual, stage[0] / Math.max(1, box[0]), stage[1] / Math.max(1, box[1]));
}

/** Keep a pan offset within what the image overhangs the stage; no overhang, no pan. */
export function clampPan(pan: Size, content: Size, stage: Size): Size {
  const limit = (i: 0 | 1) => Math.max(0, (content[i] - stage[i]) / 2);
  const clamp = (value: number, max: number) => Math.min(max, Math.max(-max, value)) || 0;
  return [clamp(pan[0], limit(0)), clamp(pan[1], limit(1))];
}

/** The divider's position (0 = left edge, 1 = right edge) for a pointer at `x` over a stage spanning `left`..`left + width`. */
export function dividerAt(x: number, left: number, width: number): number {
  if (width <= 0) return 0.5;
  return Math.min(1, Math.max(0, (x - left) / width));
}

/** An asset URL that changes when the file does, so the webview never shows a cached older result. */
export function versioned(url: string, shot: Shot): string {
  return `${url}?v=${shot.version}`;
}
