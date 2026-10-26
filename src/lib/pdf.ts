// sE pdf-formats: the PDF card's pure helpers (crop, and below, extracting pages).
import { fitRatio, type CropFrame } from './edits';
import { t } from './i18n';
import type { Edits, EditsPatch, JobUpdate } from './ipc';
import type { Device } from './settings';

/** What the PDF tools read from settings; `pdf_config` fills it. */
export interface PdfConfig {
  dpi: number;
  dpi_aggressive: number;
  extract_dpi: number;
  devices: Device[];
}

/** `ratio` turned to match the page: landscape on a landscape page, portrait on a portrait one (mirrors ferry_core::pdf::crop::ratio_box). */
export function orientRatio(dims: [number, number], ratio: [number, number]): [number, number] {
  const [long, short] = [Math.max(...ratio), Math.min(...ratio)];
  if (dims[0] > dims[1]) return [long, short];
  if (dims[0] < dims[1]) return [short, long];
  return ratio;
}

/** Where a ratio crop cuts a page: centred, as fractions of the page. */
export function pdfFrame(dims: [number, number] | null, aspect: [number, number] | null): CropFrame | null {
  if (dims === null || aspect === null) return null;
  const [fw, fh] = fitRatio(dims, orientRatio(dims, aspect));
  return {
    x: (dims[0] - fw) / 2 / dims[0],
    y: (dims[1] - fh) / 2 / dims[1],
    w: fw / dims[0],
    h: fh / dims[1],
  };
}

/** The ratio chips of the PDF crop popover. There is no 9:16: on a PDF a ratio follows each page. */
export const PDF_ASPECTS: { label: string; ratio: [number, number] }[] = [
  { label: '16:9', ratio: [16, 9] },
  { label: '4:3', ratio: [4, 3] },
  { label: '1:1', ratio: [1, 1] },
  { label: '1.91:1', ratio: [191, 100] },
];

export interface PdfCropForm {
  aspect: [number, number] | null;
  uncrop: boolean;
}

export function pdfCropForm(edits: Edits): PdfCropForm {
  return { aspect: edits.aspect, uncrop: edits.uncrop };
}

export function pdfCropPatch(form: PdfCropForm): EditsPatch {
  return { aspect: form.aspect, uncrop: form.uncrop };
}

/** The resolutions the DPI choices step through, highest first (mirrors ferry_core::pdf::DPI_STOPS). */
export const DPI_STOPS = [300, 250, 200, 150, 100, 72, 48];

export function dpiLabel(n: number): string {
  return t('pdf.dpi.label', { n });
}

/** The resolution a PDF card's images are at: its recipe, else extraction's, else the level's (mirrors ferry_core::pdf::target_dpi). */
export function currentDpi(card: JobUpdate, cfg: PdfConfig): number {
  if (card.edits.dpi !== null) return card.edits.dpi;
  if (card.format !== 'pdf') return cfg.extract_dpi;
  return card.edits.level === 'aggressive' ? cfg.dpi_aggressive : cfg.dpi;
}

/** One click on DPI: the next lower stop; null at the lowest. */
export function nextDpi(dpi: number): number | null {
  return DPI_STOPS.find((stop) => stop < dpi) ?? null;
}

export function dpiPatch(dpi: number): EditsPatch {
  return { dpi };
}

export interface PagesForm {
  format: 'png' | 'jpeg';
  all: boolean;
  from: string;
  to: string;
  dpi: number;
}

export function pagesForm(card: JobUpdate, cfg: PdfConfig): PagesForm {
  const range = card.edits.pages;
  return {
    format: card.edits.format === 'jpeg' ? 'jpeg' : 'png',
    all: range === null,
    from: String(range?.first ?? 1),
    to: String(range?.last ?? card.pages ?? 1),
    dpi: card.edits.dpi ?? cfg.extract_dpi,
  };
}

const whole = (text: string): number | null => (/^\d+$/.test(text.trim()) ? Number(text) : null);

/** The patch for extracting `form`'s pages, or why it can't be (`count` is the PDF's page count). */
export function checkPages(form: PagesForm, count: number | null): { patch: EditsPatch | null; error: string | null } {
  const patch = (pages: { first: number; last: number } | null): EditsPatch => ({ format: form.format, pages, dpi: form.dpi });
  if (form.all) return { patch: patch(null), error: null };
  const first = whole(form.from);
  const last = whole(form.to);
  if (first === null || last === null || first < 1 || last < first || (count !== null && last > count)) {
    return { patch: null, error: t('pdf.pages.invalid', { n: count ?? '?' }) };
  }
  return { patch: patch({ first, last }), error: null };
}

/** The longest side a device may have (mirrors ferry_core::edit::MAX_SIDE). */
const MAX_SIDE = 30_000;

/** The devices Settings restores (mirrors ferry_core::settings::Pdf::default). */
export const DEFAULT_DEVICES: Device[] = [
  { name: 'iPhone', width: 1179, height: 2556 },
  { name: 'iPhone Pro Max', width: 1320, height: 2868 },
  { name: 'iPad', width: 1640, height: 2360 },
  { name: 'iPad Pro 13-inch', width: 2064, height: 2752 },
  { name: 'Kindle', width: 1236, height: 1648 },
  { name: 'reMarkable', width: 1404, height: 1872 },
];

/** "iPad 1640×2360" as a device; null without a name or with a side outside 1..=30000. */
export function parseDevice(text: string): Device | null {
  const match = /^\s*(.+?)\s+(\d+)\s*[x×X]\s*(\d+)\s*$/.exec(text);
  if (!match) return null;
  const [width, height] = [Number(match[2]), Number(match[3])];
  const ok = (side: number) => side >= 1 && side <= MAX_SIDE;
  return ok(width) && ok(height) ? { name: match[1], width, height } : null;
}

export function deviceLabel(d: Device): string {
  return `${d.name} ${d.width}×${d.height}`;
}
