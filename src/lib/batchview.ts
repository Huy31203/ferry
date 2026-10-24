import { formatBytes, savingPercent } from './format';
import type {
  BatchRow,
  BatchStatus,
  MediaKind,
  RowStatus,
} from './drop-ipc';
import { t } from './i18n';

export type SortKey = 'name' | 'kind' | 'before' | 'after' | 'saved' | 'status';
export interface Filter {
  kind: MediaKind | null;
  status: RowStatus | null;
}

/** Declaration order of ferry_core::batch::RowStatus. */
export const STATUS_ORDER: RowStatus[] = [
  'waiting',
  'working',
  'done',
  'already_optimal',
  'larger',
  'skipped',
  'failed',
  'cancelled',
  'restored',
];

const FINISHED: RowStatus[] = ['done', 'already_optimal', 'larger', 'failed', 'restored'];

export function isFinished(status: RowStatus): boolean {
  return FINISHED.includes(status);
}

function sortValue(row: BatchRow, key: SortKey): string | number {
  switch (key) {
    case 'name':
      return row.name.toLowerCase();
    case 'kind':
      return row.kind ?? '';
    case 'before':
      return row.before;
    case 'after':
      return row.after ?? row.before;
    case 'saved':
      return row.before - (row.after ?? row.before);
    case 'status':
      return STATUS_ORDER.indexOf(row.status);
  }
}

/** Indices of the rows that pass `filter`, sorted by `key` with ties kept in index order. */
export function orderRows(
  rows: BatchRow[],
  key: SortKey | null,
  descending: boolean,
  filter: Filter,
): number[] {
  const kept = rows.filter(
    (r) =>
      (filter.kind === null || r.kind === filter.kind) &&
      (filter.status === null || r.status === filter.status),
  );
  if (key !== null) {
    const sign = descending ? -1 : 1;
    kept.sort((a, b) => {
      const x = sortValue(a, key);
      const y = sortValue(b, key);
      const order = x < y ? -1 : x > y ? 1 : 0;
      return order * sign || a.index - b.index;
    });
  }
  return kept.map((r) => r.index);
}

/** Replace rows by `index` and append new ones. `added` is true only when a new index appeared. */
export function mergeRows(
  rows: BatchRow[],
  changed: BatchRow[],
): { rows: BatchRow[]; added: boolean } {
  const next = rows.slice();
  let added = false;
  for (const row of changed) {
    if (row.index >= next.length) added = true;
    next[row.index] = row;
  }
  return { rows: next, added };
}

export interface Toolbar {
  pause: 'pause' | 'resume' | null;
  cancel: boolean;
  rerun: boolean;
  rollback: boolean;
  reveal: boolean;
}

export function toolbar(status: BatchStatus, rows: BatchRow[]): Toolbar {
  const { phase, tally } = status;
  const idle = phase === 'done' || phase === 'paused';
  return {
    pause: phase === 'running' ? 'pause' : phase === 'paused' ? 'resume' : null,
    cancel: phase === 'running' || phase === 'paused',
    rerun: idle && rows.some((r) => isFinished(r.status)),
    rollback: idle && tally.done > 0,
    reveal: rows.some((r) => r.output !== null),
  };
}

function eta(secs: number): string {
  return secs < 120
    ? t('drop.batch.eta.seconds', { n: secs })
    : t('drop.batch.eta.minutes', { n: Math.ceil(secs / 60) });
}

export function statusLine(status: BatchStatus, locale?: string): string {
  const { tally } = status;
  const done = tally.total - tally.waiting - tally.working;
  switch (status.phase) {
    case 'running':
      return status.eta_secs === null
        ? t('drop.batch.running_no_eta', { done, total: tally.total })
        : t('drop.batch.running', {
            done,
            total: tally.total,
            rate: status.rate.toFixed(1),
            eta: eta(status.eta_secs),
          });
    case 'paused':
      return t('drop.batch.paused', { done, total: tally.total });
    case 'rolling_back': {
      const [at, total] = status.rolling ?? [0, 0];
      return t('drop.batch.rolling_back', { done: at, total });
    }
    case 'done':
      return t('drop.batch.summary', {
        count: new Intl.NumberFormat(locale).format(tally.total),
        before: formatBytes(tally.before, locale),
        after: formatBytes(tally.after, locale),
        percent: savingPercent(tally.before, tally.after),
      });
    case 'empty':
      return '';
  }
}
