<script lang="ts">
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { startDrag } from '@crabnebula/tauri-plugin-drag';
  import { t, type StringKey } from '../lib/i18n';
  import { formatBytes, savingPercent } from '../lib/format';
  import { isMac } from '../lib/ipc';
  import { visibleRange } from '../lib/virtual';
  import { orderRows, type Filter, type SortKey } from '../lib/batchview';
  import type { BatchRow, MediaKind, RowStatus } from '../lib/drop-ipc';

  const ROW = 32;
  const KINDS: MediaKind[] = ['image', 'video', 'audio', 'pdf'];
  const STATUSES: RowStatus[] = [
    'waiting', 'working', 'done', 'already_optimal', 'larger', 'skipped', 'failed', 'cancelled', 'restored',
  ];
  const ICONS: Record<RowStatus, string> = {
    waiting: '…', working: '◐', done: '✓', already_optimal: '✓', larger: '↩',
    skipped: '–', failed: '!', cancelled: '×', restored: '↺',
  };
  const COLUMNS: [SortKey, StringKey][] = [
    ['name', 'drop.batch.col.name'],
    ['kind', 'drop.batch.col.kind'],
    ['before', 'drop.batch.col.before'],
    ['after', 'drop.batch.col.after'],
    ['saved', 'drop.batch.col.saved'],
  ];

  let { rows, added, status: statusFilter = $bindable(null) }: { rows: BatchRow[]; added: number; status?: RowStatus | null } = $props();

  let sortKey = $state<SortKey | null>(null);
  let descending = $state(false);
  let kind = $state<MediaKind | null>(null);
  let scrollTop = $state(0);
  let viewport = $state(400);
  let cursor = $state(0);
  let checked = $state(new Set<number>());

  // The order is recomputed when the user sorts or filters, or rows are added, never on a status change.
  let order = $state<number[]>([]);
  $effect(() => {
    added;
    const filter: Filter = { kind, status: statusFilter };
    order = orderRows(rows, sortKey, descending, filter);
  });

  const range = $derived(visibleRange(scrollTop, viewport, ROW, order.length));
  const shown = $derived(order.slice(range.start, range.end));

  function sortBy(key: SortKey) {
    if (sortKey === key) descending = !descending;
    else {
      sortKey = key;
      descending = false;
    }
  }

  function toggle(index: number) {
    const next = new Set(checked);
    if (!next.delete(index)) next.add(index);
    checked = next;
  }

  function saved(row: BatchRow): string {
    return row.after === null ? '' : savingPercent(row.before, row.after);
  }

  function size(dims: [number, number] | null): string {
    return dims ? `${dims[0]}×${dims[1]}` : '';
  }

  function pointerdown(event: MouseEvent, row: BatchRow) {
    if (event.button !== 0 || (event.target as HTMLElement).closest('input')) return;
    const files = checked.has(row.index)
      ? rows.filter((r) => checked.has(r.index) && r.output).map((r) => r.output as string)
      : row.output
        ? [row.output]
        : [];
    if (files.length === 0) return;
    startDrag({ item: files, icon: row.thumb ?? '' }).catch((e) => console.warn('batch drag', e));
  }

  const open = (action: 'open_compare' | 'quick_preview', row: BatchRow) =>
    row.job && invoke(action, { id: row.job }).catch((e) => console.warn(action, e));

  function keydown(event: KeyboardEvent) {
    const mod = isMac ? event.metaKey : event.ctrlKey;
    const row = rows[order[cursor]];
    if (mod && event.key.toLowerCase() === 'a') {
      checked = new Set(order);
    } else if (event.key === 'Escape') {
      checked = new Set();
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      cursor = Math.max(0, Math.min(order.length - 1, cursor + (event.key === 'ArrowDown' ? 1 : -1)));
      if (event.shiftKey) checked = new Set(checked).add(order[cursor]);
    } else if (event.key === ' ' && row) {
      open('quick_preview', row);
    } else if (mod && event.key.toLowerCase() === 'd' && row) {
      open('open_compare', row);
    } else return;
    event.preventDefault();
  }
</script>

<div class="table" role="grid" aria-label={t('drop.batch.aria')} aria-rowcount={order.length} tabindex="0" onkeydown={keydown}>
  <div class="head" role="row">
    <span class="c-check"></span>
    <span class="c-thumb"></span>
    {#each COLUMNS as [key, label] (key)}
      <button type="button" class={`c-${key}`} role="columnheader" onclick={() => sortBy(key)}>
        {t(label)}{sortKey === key ? (descending ? ' ▼' : ' ▲') : ''}
      </button>
    {/each}
    <span class="c-res" role="columnheader">{t('drop.batch.col.resolution')}</span>
    <span class="c-status" role="columnheader">{t('drop.batch.col.status')}</span>
  </div>
  <div class="filters">
    <select value={kind ?? ''} onchange={(e) => (kind = (e.currentTarget.value || null) as MediaKind | null)}>
      <option value="">{t('drop.batch.filter.all_kinds')}</option>
      {#each KINDS as k (k)}<option value={k}>{t(`drop.kind.${k}`)}</option>{/each}
    </select>
    <select value={statusFilter ?? ''} onchange={(e) => (statusFilter = (e.currentTarget.value || null) as RowStatus | null)}>
      <option value="">{t('drop.batch.filter.all_statuses')}</option>
      {#each STATUSES as s (s)}<option value={s}>{t(`drop.batch.status.${s}`)}</option>{/each}
    </select>
  </div>
  <div class="body" bind:clientHeight={viewport} onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}>
    <div style:padding-top="{range.padTop}px" style:padding-bottom="{range.padBottom}px">
      {#each shown as index, i (index)}
        {@const row = rows[index]}
        <div
          class="row"
          class:cursor={range.start + i === cursor}
          role="row"
          tabindex="-1"
          aria-rowindex={range.start + i + 1}
          style:height="{ROW}px"
          onmousedown={(e) => pointerdown(e, row)}
          ondblclick={() => open('quick_preview', row)}
          oncontextmenu={(e) => {
            e.preventDefault();
            if (row.job) invoke('batch_row_menu', { id: row.job, x: e.clientX, y: e.clientY }).catch((err) => console.warn(err));
          }}
        >
          <input class="c-check" type="checkbox" checked={checked.has(index)} aria-label={t('drop.batch.select', { name: row.name })} onchange={() => toggle(index)} />
          <span class="c-thumb">
            {#if row.thumb}<img src={convertFileSrc(row.thumb)} alt="" width="28" height="28" loading="lazy" decoding="async" />{/if}
          </span>
          <span class="c-name" title={row.path}>{row.name}.{row.ext}</span>
          <span class="c-kind">{row.kind ? t(`drop.kind.${row.kind}`) : ''}</span>
          <span class="c-before">{formatBytes(row.before)}</span>
          <span class="c-after">{row.after === null ? '' : formatBytes(row.after)}</span>
          <span class="c-saved">{saved(row)}</span>
          <span class="c-res">{size(row.dims_before)}{row.dims_after ? ` → ${size(row.dims_after)}` : ''}</span>
          <span class={`c-status s-${row.status}`} title={row.reason ?? ''}>
            <span aria-hidden="true">{ICONS[row.status]}</span>
            {t(`drop.batch.status.${row.status}`)}
            {#if row.status === 'working' && row.progress > 0}<progress value={row.progress} max="1"></progress>{/if}
          </span>
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .table {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
    outline: none;
  }
  .head,
  .row {
    display: grid;
    grid-template-columns: 28px 36px minmax(120px, 1fr) 70px 80px 80px 70px 150px 150px;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
  }
  .head {
    height: 28px;
    color: var(--fg-2);
    font-size: 12px;
    border-bottom: 1px solid var(--rail-line);
  }
  .head button {
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    padding: 0;
    cursor: pointer;
  }
  .filters {
    display: flex;
    gap: 8px;
    padding: 4px 10px;
  }
  .body {
    flex: 1;
    overflow-y: auto;
  }
  .row {
    box-sizing: border-box;
    border-bottom: 1px solid color-mix(in srgb, var(--rail-line) 40%, transparent);
    user-select: none;
  }
  .row.cursor {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .row span {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  img {
    border-radius: 3px;
    object-fit: cover;
    display: block;
  }
  .s-failed {
    color: var(--error);
  }
  .s-done,
  .s-already_optimal {
    color: var(--accent-text);
  }
  progress {
    width: 50px;
    vertical-align: middle;
  }
</style>
