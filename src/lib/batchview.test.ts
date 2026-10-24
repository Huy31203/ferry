import { describe, expect, it } from 'vitest';
import type { BatchRow, BatchStatus, Tally } from './drop-ipc';
import { mergeRows, orderRows, statusLine, toolbar } from './batchview';

const row = (index: number, over: Partial<BatchRow> = {}): BatchRow => ({
  index,
  path: `/d/${index}.png`,
  name: String(index),
  ext: 'png',
  kind: 'image',
  before: 100,
  after: null,
  dims_before: null,
  dims_after: null,
  status: 'waiting',
  reason: null,
  progress: 0,
  step: null,
  job: null,
  thumb: null,
  output: null,
  can_restore: false,
  can_edit: false,
  keep_original: false,
  ...over,
});

const tally = (over: Partial<Tally> = {}): Tally => ({
  total: 0,
  waiting: 0,
  working: 0,
  done: 0,
  already_optimal: 0,
  larger: 0,
  skipped: 0,
  failed: 0,
  cancelled: 0,
  restored: 0,
  before: 0,
  after: 0,
  ...over,
});

const status = (phase: BatchStatus['phase'], over: Partial<Tally> = {}, extra = {}): BatchStatus => ({
  phase,
  tally: tally(over),
  rate: 0,
  eta_secs: null,
  rolling: null,
  skipped_in_folders: 0,
  truncated: false,
  ...extra,
});

describe('batch view', () => {
  it('sorts by saving and filters by status without moving ties', () => {
    const rows = [
      row(0, { status: 'done', after: 90 }),
      row(1, { status: 'done', after: 40 }),
      row(2, { status: 'failed', after: null }),
      row(3, { status: 'done', after: 90 }),
    ];
    const none = { kind: null, status: null };
    expect(orderRows(rows, null, false, none)).toEqual([0, 1, 2, 3]);
    expect(orderRows(rows, 'saved', true, none)).toEqual([1, 0, 3, 2]);
    expect(orderRows(rows, 'saved', false, { kind: null, status: 'done' })).toEqual([0, 3, 1]);
    expect(orderRows(rows, 'status', false, none)).toEqual([0, 1, 3, 2]);
  });

  it('merges changed rows in place and says when rows were added', () => {
    const rows = [row(0), row(1)];
    const same = mergeRows(rows, [row(1, { status: 'done' })]);
    expect([same.added, same.rows[1].status, same.rows.length]).toEqual([false, 'done', 2]);
    expect(mergeRows(rows, [row(2)]).added).toBe(true);
  });

  it('offers pause, resume, rerun and roll back only when they apply', () => {
    const rows = [row(0, { status: 'done', output: '/o' })];
    expect(toolbar(status('running'), rows)).toMatchObject({ pause: 'pause', cancel: true, rerun: false });
    expect(toolbar(status('paused'), rows)).toMatchObject({ pause: 'resume', rerun: true, reveal: true });
    expect(toolbar(status('done', { done: 1 }), rows)).toMatchObject({ pause: null, cancel: false, rerun: true, rollback: true });
    expect(toolbar(status('empty'), [])).toMatchObject({ rerun: false, rollback: false, reveal: false });
  });

  it('reads like the spec when done', () => {
    const done = status('done', { total: 1204, before: 2_100_000_000, after: 640_000_000 });
    expect(statusLine(done, 'en-US')).toBe('1,204 files · 2.1 GB → 640 MB · −70%');
  });

  it('shows done of total, rate and time left while running', () => {
    const running = status('running', { total: 100, waiting: 40, working: 10 }, { rate: 2.5, eta_secs: 20 });
    expect(statusLine(running)).toBe('50 of 100 · 2.5 files/s · 20 s left');
    const slow = { ...running, eta_secs: 300 };
    expect(statusLine(slow)).toBe('50 of 100 · 2.5 files/s · 5 min left');
    expect(statusLine({ ...running, eta_secs: null })).toBe('50 of 100');
  });
});
