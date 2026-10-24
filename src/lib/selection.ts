import { formatBytes, savingPercent } from './format';
import { t } from './i18n';
import type { JobUpdate } from './ipc';

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
