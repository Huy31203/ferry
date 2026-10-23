import { t } from './i18n';
import type { Edits, EditsPatch, Format, JobUpdate, Size } from './ipc';

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

/** The largest width, height or long edge a crop may ask for (`ferry_core::edit::MAX_SIDE`). */
export const MAX_SIDE = 30_000;

/** The crop popover's ratio chips; `null` is "Free" (no ratio crop, only the size fields). */
export const ASPECTS: { label: string; ratio: [number, number] | null }[] = [
  { label: '', ratio: null },
  { label: '16:9', ratio: [16, 9] },
  { label: '4:3', ratio: [4, 3] },
  { label: '1:1', ratio: [1, 1] },
  { label: '1.91:1', ratio: [191, 100] },
  { label: '9:16', ratio: [9, 16] },
];

/** What the crop popover's fields hold, as typed. */
export interface CropForm {
  aspect: [number, number] | null;
  mode: 'size' | 'long_edge';
  width: string;
  height: string;
  edge: string;
  framing: 'centre' | 'smart';
}

/** The popover's starting point: the card's current crop, if any. */
export function cropForm(edits: Edits): CropForm {
  const size = edits.size;
  return {
    aspect: edits.aspect,
    mode: size?.kind === 'long_edge' ? 'long_edge' : 'size',
    width: size?.kind === 'exact' || size?.kind === 'width' ? String(size.width) : '0',
    height: size?.kind === 'exact' || size?.kind === 'height' ? String(size.height) : '0',
    edge: size?.kind === 'long_edge' ? String(size.edge) : '0',
    framing: edits.framing,
  };
}

export type CropField = 'width' | 'height' | 'edge';

export type CropCheck =
  | { patch: EditsPatch; larger: boolean; error: null; field: null }
  | { patch: null; larger: false; error: string; field: CropField | null };

/** A field as a side: '' is 0; anything but digits, or more than MAX_SIDE, is an error. */
function side(text: string): number | 'not_number' | 'too_large' {
  const trimmed = text.trim();
  if (trimmed === '') return 0;
  if (!/^\d+$/.test(trimmed)) return 'not_number';
  const value = Number(trimmed);
  return value > MAX_SIDE ? 'too_large' : value;
}

/** Check the popover (spec §5.3) and build the `card_edit` patch: aspect, framing and size. */
export function checkCrop(form: CropForm, dims: [number, number] | null): CropCheck {
  const fields: CropField[] = form.mode === 'size' ? ['width', 'height'] : ['edge'];
  const values: Record<CropField, number> = { width: 0, height: 0, edge: 0 };
  for (const field of fields) {
    const value = side(form[field]);
    if (typeof value === 'string') {
      const error = t(value === 'not_number' ? 'crop.not_number' : 'crop.too_large');
      return { patch: null, larger: false, error, field };
    }
    values[field] = value;
  }
  const { width, height, edge } = values;
  let size: Size | null = null;
  if (form.mode === 'long_edge') {
    if (edge > 0) size = { kind: 'long_edge', edge };
  } else if (width > 0 && height > 0) {
    size = { kind: 'exact', width, height };
  } else if (width > 0) {
    size = { kind: 'width', width };
  } else if (height > 0) {
    size = { kind: 'height', height };
  }
  if (size === null && form.aspect === null) {
    return { patch: null, larger: false, error: t('crop.nothing'), field: null };
  }
  const larger =
    dims !== null &&
    (width > dims[0] || height > dims[1] || edge > Math.max(dims[0], dims[1]));
  return {
    patch: { aspect: form.aspect, framing: form.framing, size },
    larger,
    error: null,
    field: null,
  };
}

/** ×0.5 and ×2: every non-zero size field, rounded, kept within 1..MAX_SIDE; fields that are not numbers stay. */
export function scaleForm(form: CropForm, factor: number): CropForm {
  const scale = (text: string) => {
    const value = side(text);
    if (typeof value === 'string' || value === 0) return text;
    return String(Math.min(MAX_SIDE, Math.max(1, Math.round(value * factor))));
  };
  return { ...form, width: scale(form.width), height: scale(form.height), edge: scale(form.edge) };
}

/** The largest window of `ratio` inside `dims`, rounded, at least 1×1 (mirrors `ferry_core::edit::geometry::fit_ratio`). */
export function fitRatio([w, h]: [number, number], [rw, rh]: [number, number]): [number, number] {
  if (w * rh > h * rw) {
    return [Math.min(w, Math.max(1, Math.floor((h * rw + Math.floor(rh / 2)) / rh))), h];
  }
  return [w, Math.min(h, Math.max(1, Math.floor((w * rh + Math.floor(rw / 2)) / rw)))];
}

/** Where the crop cuts, as fractions of the original: centred (Smart framing is decided by Rust and not previewed). */
export interface CropFrame {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function cropFrame(dims: [number, number] | null, form: CropForm): CropFrame | null {
  if (dims === null) return null;
  const check = checkCrop(form, dims);
  const size = check.patch?.size;
  const ratio: [number, number] | null =
    size?.kind === 'exact' ? [size.width, size.height] : (check.patch?.aspect ?? null);
  if (ratio === null) return null;
  const [fw, fh] = fitRatio(dims, ratio);
  return {
    x: (dims[0] - fw) / 2 / dims[0],
    y: (dims[1] - fh) / 2 / dims[1],
    w: fw / dims[0],
    h: fh / dims[1],
  };
}

/** `frame` in card pixels, over a thumbnail drawn with `object-fit: contain` in a `box`-sized card. */
export function frameBox(
  frame: CropFrame,
  dims: [number, number],
  box: [number, number],
): { left: number; top: number; width: number; height: number } {
  const scale = Math.min(box[0] / dims[0], box[1] / dims[1]);
  const [iw, ih] = [dims[0] * scale, dims[1] * scale];
  const [ix, iy] = [(box[0] - iw) / 2, (box[1] - ih) / 2];
  return { left: ix + frame.x * iw, top: iy + frame.y * ih, width: frame.w * iw, height: frame.h * ih };
}

/** Fit under size chips, in MB. */
export const FIT_PRESETS_MB = [1, 5, 10, 25];

/** The fit field in bytes (decimal units, like the cards), or null when it is not a size above 0. */
export function fitBytes(text: string, unit: 'kb' | 'mb'): number | null {
  const trimmed = text.trim();
  if (!/^\d+(\.\d+)?$/.test(trimmed)) return null;
  const bytes = Math.round(Number(trimmed) * (unit === 'mb' ? 1_000_000 : 1_000));
  return bytes > 0 ? bytes : null;
}

/** The fit field's starting text and unit: the card's current target, or empty in MB. */
export function fitForm(fitUnder: number | null): { text: string; unit: 'kb' | 'mb' } {
  if (fitUnder === null) return { text: '', unit: 'mb' };
  return fitUnder % 1_000_000 === 0
    ? { text: String(fitUnder / 1_000_000), unit: 'mb' }
    : { text: String(Math.round(fitUnder / 1_000)), unit: 'kb' };
}

/** A saved crop size as typed in Settings: "1200×628", "1200x628" or "1200 x 628"; each side 1..MAX_SIDE. */
export function parseSize(text: string): [number, number] | null {
  const match = /^\s*(\d+)\s*[x×X]\s*(\d+)\s*$/.exec(text);
  if (!match) return null;
  const [w, h] = [Number(match[1]), Number(match[2])];
  return w >= 1 && h >= 1 && w <= MAX_SIDE && h <= MAX_SIDE ? [w, h] : null;
}

/** "1200×628". */
export function sizeLabel([w, h]: [number, number]): string {
  return `${w}×${h}`;
}

/** The saved sizes with `size` first (newest first), without a duplicate. */
export function addSize(sizes: [number, number][], size: [number, number]): [number, number][] {
  return [size, ...sizes.filter(([w, h]) => w !== size[0] || h !== size[1])];
}
