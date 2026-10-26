import { canDrag } from './cards';
import { formatBytes, savingPercent } from './format';
import { t } from './i18n';
import type { EditsPatch, JobUpdate } from './ipc';
import { dpiPatch, nextDpi, type PdfConfig } from './pdf';

/** Height of a row in the compact list, in px. */
export const LIST_ROW = 44;
/** The list shows this many rows at a time and scrolls. */
export const LIST_ROWS_MAX = 8;

/** Cards become a list above `after`; the layout never flips while the user is working in it. */
export function nextListMode(count: number, after: number, current: boolean, held: boolean): boolean {
  if (count === 0) return false;
  if (held) return current;
  return count > after;
}

/** Rows stay put while pointed at, selected, being edited or dragged. */
export function rowsHeld(overList: boolean, selected: number, tool: boolean, dragging: boolean): boolean {
  return overList || selected > 0 || tool || dragging;
}

export interface Totals {
  count: number;
  before: number;
  after: number;
  saved: number;
}

/** What the finished cards saved, counting only Done ones with a result size. */
export function listTotals(cards: JobUpdate[]): Totals {
  let before = 0;
  let after = 0;
  for (const card of cards) {
    if (card.state.kind === 'done' && card.after !== null) {
      before += card.before;
      after += card.after;
    }
  }
  return { count: cards.length, before, after, saved: before - after };
}

/** "23 files · 41.2 MB saved (−68%)", or just the count when nothing was saved. */
export function headerText(totals: Totals, locale?: string): string {
  const count = t(totals.count === 1 ? 'drop.list.count.one' : 'drop.list.count.other', {
    n: new Intl.NumberFormat(locale).format(totals.count),
  });
  if (totals.saved <= 0) return count;
  return t('drop.list.header', {
    count,
    saved: formatBytes(totals.saved, locale),
    percent: savingPercent(totals.before, totals.after),
  });
}

/** The selected cards, the end the next Shift-move extends from, and the keyboard cursor. */
export interface Sel {
  ids: string[];
  anchor: string | null;
  cursor: string | null;
}

export const NO_SELECTION: Sel = { ids: [], anchor: null, cursor: null };

/** Add or remove one card; it becomes the anchor and the cursor. */
export function toggle(sel: Sel, id: string): Sel {
  const ids = sel.ids.includes(id) ? sel.ids.filter((x) => x !== id) : [...sel.ids, id];
  return { ids, anchor: id, cursor: id };
}

export function selectAll(sel: Sel, cards: JobUpdate[]): Sel {
  return { ...sel, ids: cards.map((c) => c.id) };
}

/** Nothing selected; the cursor stays where it was. */
export function clearSel(sel: Sel): Sel {
  return { ids: [], anchor: null, cursor: sel.cursor };
}

/** Move the cursor one card; with `extend`, select the range from the anchor to it, in card order. */
export function moveCursor(sel: Sel, cards: JobUpdate[], delta: 1 | -1, extend: boolean): Sel {
  if (cards.length === 0) return sel;
  const at = cards.findIndex((c) => c.id === sel.cursor);
  const next = at === -1 ? (delta === 1 ? cards.length - 1 : 0) : Math.min(cards.length - 1, Math.max(0, at + delta));
  const cursor = cards[next].id;
  if (!extend) return { ...sel, anchor: cursor, cursor };
  const anchor = sel.anchor ?? sel.cursor ?? cursor;
  const from = cards.findIndex((c) => c.id === anchor);
  const [lo, hi] = from === -1 ? [next, next] : [Math.min(from, next), Math.max(from, next)];
  return { ids: cards.slice(lo, hi + 1).map((c) => c.id), anchor, cursor };
}

/** Shift-click: select the range from the anchor to `id`, in card order. */
export function extendTo(sel: Sel, cards: JobUpdate[], id: string): Sel {
  const anchor = sel.anchor ?? sel.cursor;
  const from = cards.findIndex((c) => c.id === anchor);
  const to = cards.findIndex((c) => c.id === id);
  if (from === -1 || to === -1) return toggle(sel, id);
  const [lo, hi] = [Math.min(from, to), Math.max(from, to)];
  return { ids: cards.slice(lo, hi + 1).map((c) => c.id), anchor, cursor: id };
}

/** Forget cards that are gone. */
export function prune(sel: Sel, cards: JobUpdate[]): Sel {
  const have = new Set(cards.map((c) => c.id));
  const ids = sel.ids.filter((id) => have.has(id));
  const keep = (id: string | null) => (id !== null && have.has(id) ? id : null);
  if (ids.length === sel.ids.length && keep(sel.anchor) === sel.anchor && keep(sel.cursor) === sel.cursor) {
    return sel;
  }
  return { ids, anchor: keep(sel.anchor), cursor: keep(sel.cursor) };
}

export interface Targets {
  /** Output files of the selected cards that can be dragged, copied or saved. */
  files: string[];
  fileIds: string[];
  /** Selected cards the edit tools apply to. */
  edit: string[];
  cantEdit: number;
}

export function targets(cards: JobUpdate[], ids: string[]): Targets {
  const chosen = cards.filter((c) => ids.includes(c.id));
  const files = chosen.filter(canDrag);
  const edit = chosen.filter((c) => c.can_edit);
  return {
    files: files.map((c) => c.output as string),
    fileIds: files.map((c) => c.id),
    edit: edit.map((c) => c.id),
    cantEdit: chosen.length - edit.length,
  };
}

/** The parts of a KeyboardEvent the list reads. */
export interface KeyPress {
  key: string;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
}

export type ListKey =
  | { kind: 'select_all' }
  | { kind: 'escape' }
  | { kind: 'move'; delta: 1 | -1; extend: boolean }
  | { kind: 'preview' }
  | { kind: 'compare' }
  | { kind: 'step_down' }
  | { kind: 'scale'; percent: number };

/** The list's keys, with the platform's command key (Cmd on macOS, Ctrl elsewhere). Any other key is left alone. */
export function listKey(e: KeyPress, mac: boolean): ListKey | null {
  const command = mac ? e.metaKey : e.ctrlKey;
  const other = mac ? e.ctrlKey : e.metaKey;
  if (e.altKey || other) return null;
  if (command) {
    if (e.shiftKey) return null;
    const key = e.key.toLowerCase();
    if (key === 'a') return { kind: 'select_all' };
    if (key === 'd') return { kind: 'compare' };
    return null;
  }
  if (e.key === 'Escape') return { kind: 'escape' };
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    return { kind: 'move', delta: e.key === 'ArrowDown' ? 1 : -1, extend: e.shiftKey };
  }
  if (e.shiftKey) return null;
  if (e.key === ' ') return { kind: 'preview' };
  if (e.key === '-') return { kind: 'step_down' };
  if (/^[1-9]$/.test(e.key)) return { kind: 'scale', percent: Number(e.key) * 10 };
  return null;
}

/** Esc clears a selection; with none, it hands the keyboard back. */
export function escapeSel(sel: Sel): { sel: Sel; release: boolean } {
  return sel.ids.length > 0 ? { sel: clearSel(sel), release: false } : { sel, release: true };
}

/** The scale `-` steps to: 10 points below the current one, `null` when already there (as `hover_keys::step_down`). */
export function stepDown(scale: number | null): number | null {
  const next = Math.max(10, (scale ?? 100) - 10);
  return next === scale ? null : next;
}

/** What `-` and the digits do to `card`, as `hover_keys` does: an image scales; a PDF has no scale, so `-` steps its DPI and digits do nothing. */
export function keyPatch(key: { kind: 'scale'; percent: number } | { kind: 'step_down' }, card: JobUpdate, cfg: PdfConfig): EditsPatch | null {
  if (card.source_format === 'pdf') {
    if (key.kind === 'scale') return null;
    const dpi = nextDpi(card.edits.dpi ?? (card.edits.level === 'aggressive' ? cfg.dpi_aggressive : cfg.dpi));
    return dpi === null ? null : dpiPatch(dpi);
  }
  const scale = key.kind === 'scale' ? key.percent : stepDown(card.edits.scale);
  return scale === null ? null : { scale };
}
