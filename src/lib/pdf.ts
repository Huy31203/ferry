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
