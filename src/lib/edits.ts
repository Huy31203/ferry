import { t } from './i18n';
import type { Edits, EditsPatch, Format, JobUpdate } from './ipc';

/** A Compression stop: an encoder quality 5–100, or Extreme (aggressive at quality 5). */
export type Quality = number | 'extreme';

/** Downscale slider stops, top to bottom: 100 % (no downscale), then 90 % … 10 % in steps of 5. */
export const SCALE_STOPS: number[] = [100, ...Array.from({ length: 17 }, (_, i) => 90 - i * 5)];

/** Compression slider stops, top to bottom: quality 100 … 5 in steps of 5, then Extreme. */
export const QUALITY_STOPS: Quality[] = [
  ...Array.from({ length: 20 }, (_, i) => 100 - i * 5),
  'extreme',
];

/** One click on Downscale: 10 points below the current scale (100 → 90 → … → 10). */
export function nextScale(scale: number | null): number {
  return Math.max(10, (scale ?? 100) - 10);
}

/** The card's quality: its own, else the settings' `fallback`. */
export function currentQuality(edits: Edits, fallback: number): Quality {
  if (edits.level === 'aggressive' && edits.compression === 5) return 'extreme';
  return edits.compression ?? fallback;
}

/** One click on Compression: 10 quality lower, then Extreme. */
export function nextQuality(quality: Quality): Quality {
  if (quality === 'extreme' || quality - 10 < 5) return 'extreme';
  return quality - 10;
}

export function scalePatch(scale: number): EditsPatch {
  return { scale: scale >= 100 ? null : scale };
}

export function qualityPatch(quality: Quality): EditsPatch {
  return quality === 'extreme'
    ? { compression: 5, level: 'aggressive' }
    : { compression: quality, level: 'normal' };
}

/** `side × percent`, rounded like ferry_core's `scaled`, at least 1. */
function scaled(side: number, percent: number): number {
  return Math.max(1, Math.round((side * percent) / 100));
}

/** "50 % · 1440×900"; just "50 %" when a crop changes the size first or the size is unknown. */
export function scaleLabel(scale: number, dims: [number, number] | null, edits: Edits): string {
  if (!dims || edits.aspect !== null) return t('slider.scale_only', { n: scale });
  return t('slider.scale', { n: scale, w: scaled(dims[0], scale), h: scaled(dims[1], scale) });
}

/** "q 60" or "Extreme". */
export function qualityLabel(quality: Quality): string {
  return quality === 'extreme' ? t('slider.extreme') : t('slider.quality', { q: quality });
}

/** The stop a press-and-drag lands on: `start` moved by the drag, one stop per `track / (count - 1)` px, kept in range. */
export function sliderIndex(start: number, dy: number, count: number, track: number): number {
  const step = track / Math.max(1, count - 1);
  return Math.min(count - 1, Math.max(0, start + Math.round(dy / step)));
}

/** Where a stop list holds `value`; the nearest stop below it when it falls between stops. */
export function stopIndex<T extends number | string>(stops: T[], value: T): number {
  const exact = stops.indexOf(value);
  if (exact !== -1 || typeof value !== 'number') return Math.max(0, exact);
  const below = stops.findIndex((stop) => typeof stop === 'number' && stop <= value);
  return below === -1 ? stops.length - 1 : below;
}

/** A chip in the format bar (spec §5.4). */
export interface FormatChoice {
  format: Format;
  /** The format's own name, as `ferry_core::sniff::Format::label` writes it. */
  label: string;
  current: boolean;
  /** Converting a GIF keeps only its first frame. */
  firstFrame: boolean;
}

const FORMAT_LABELS: Record<Format, string> = { png: 'PNG', jpeg: 'JPEG', gif: 'GIF', webp: 'WebP' };

/** PNG · JPEG · WebP, with GIF first on a card that started as a GIF. */
export function formatChoices(card: JobUpdate): FormatChoice[] {
  const source = card.source_format ?? card.format;
  const formats: Format[] = source === 'gif' ? ['gif', 'png', 'jpeg', 'webp'] : ['png', 'jpeg', 'webp'];
  return formats.map((format) => ({
    format,
    label: FORMAT_LABELS[format],
    current: card.format === format,
    firstFrame: source === 'gif' && format !== 'gif',
  }));
}

/** What a chip click asks `card_convert` for; null for a plain click on the current format. */
export function convertRequest(
  card: JobUpdate,
  format: Format,
  keepBoth: boolean,
): { format: Format; keepBoth: boolean } | null {
  if (card.format === format && !keepBoth) return null;
  return { format, keepBoth };
}
