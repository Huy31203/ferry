import type { EditsPatch, Format, JobUpdate } from './ipc';

export const VIDEO_FORMATS: Format[] = ['mp4', 'hevc', 'av1', 'webm', 'mov', 'mkv'];

export function isVideo(format: Format | null): boolean {
  return format !== null && VIDEO_FORMATS.includes(format);
}

/** A duration as shown on the card's badge: "0:05", "1:02", "1:02:03" (whole seconds, rounded down). */
export function formatDuration(ms: number): string {
  const total = Math.floor(ms / 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = String(total % 60).padStart(2, '0');
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, '0')}:${seconds}` : `${minutes}:${seconds}`;
}

/** The speed popover's chips, in percent. */
export const SPEED_CHIPS: number[] = [125, 150, 200, 300];

/** 125 → "1.25×", 150 → "1.5×", 200 → "2×". */
export function speedLabel(percent: number): string {
  return `${percent / 100}×`;
}

/** What the custom field holds, as percent: "1.5", "1,5", "2x" and "2×" are fine; outside 0.5 to 4 is not. */
export function parseSpeed(text: string): number | null {
  const plain = text.trim().replace(/[x×]$/i, '').replace(',', '.');
  if (!/^\d+(\.\d+)?$/.test(plain)) return null;
  const percent = Math.round(Number(plain) * 100);
  return percent >= 50 && percent <= 400 ? percent : null;
}

/** What `card_edit` gets for a speed; no speed, or 100 %, clears it. */
export function speedPatch(percent: number | null, keepFrames: boolean): EditsPatch {
  if (percent === null || percent === 100) return { speed: null };
  return { speed: { percent, keep_frames: keepFrames } };
}

/** The card's extension chip: "mp4 · HEVC" on a video, the extension on an image. */
export function extLabel(card: JobUpdate): string {
  return card.video ? `${card.ext} · ${card.video.codec}` : card.ext;
}

/** The progress ring's circumference (r = 24). */
export const RING = 2 * Math.PI * 24;

/** `stroke-dasharray` for a ring filled to `progress` (0 to 1). */
export function ringDash(progress: number): string {
  return `${(progress * RING).toFixed(1)} ${RING.toFixed(1)}`;
}
