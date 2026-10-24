<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { t } from '../lib/i18n';
  import { mergeRows, statusLine, toolbar } from '../lib/batchview';
  import type { Adjust, BatchOpen, BatchRow, BatchStatus, RowStatus, Snapshot } from '../lib/drop-ipc';
  import AdjustPanel from './AdjustPanel.svelte';
  import BatchTable from './BatchTable.svelte';

  const EMPTY: BatchStatus = {
    phase: 'empty',
    tally: {
      total: 0, waiting: 0, working: 0, done: 0, already_optimal: 0, larger: 0,
      skipped: 0, failed: 0, cancelled: 0, restored: 0, before: 0, after: 0,
    },
    rate: 0,
    eta_secs: null,
    rolling: null,
    skipped_in_folders: 0,
    truncated: false,
  };

  let rows = $state<BatchRow[]>([]);
  let added = $state(0);
  let status = $state<BatchStatus>(EMPTY);
  let adjust = $state<Adjust>({ compression: null, scale: null, format: null, extreme: false });
  let threshold = $state(20);
  let adjusting = $state(false);
  let statusFilter = $state<RowStatus | null>(null);

  const bar = $derived(toolbar(status, rows));
  const total = $derived(status.tally.total);
  const progress = $derived(total === 0 ? 0 : (total - status.tally.waiting - status.tally.working) / total);

  function merge(changed: BatchRow[]) {
    const next = mergeRows(rows, changed);
    rows = next.rows;
    if (next.added) added += 1;
  }

  function setStatus(next: BatchStatus) {
    // A cleared batch (the window was closed while idle) leaves no rows behind.
    if (next.tally.total === 0 && rows.length > 0) rows = [];
    status = next;
  }

  const call = (name: string, args?: Record<string, unknown>) =>
    invoke(name, args).catch((e) => console.warn(name, e));

  function change(next: Adjust) {
    adjust = next;
    void call('batch_set_adjust', { adjust: next });
  }

  onMount(() => {
    // Listen first, then ask for the snapshot, so nothing falls in between.
    const unlisten = Promise.all([
      listen<BatchRow[]>('batch:rows', (e) => merge(e.payload)),
      listen<BatchStatus>('batch:status', (e) => setStatus(e.payload)),
      listen<BatchOpen>('batch:open', (e) =>
        call(e.payload.action === 'compare' ? 'open_compare' : 'quick_preview', { id: e.payload.id }),
      ),
    ]).then(async (stops) => {
      const snapshot = await invoke<Snapshot>('batch_ready');
      rows = snapshot.rows;
      added += 1;
      status = snapshot.status;
      adjust = snapshot.adjust;
      threshold = snapshot.threshold;
      return stops;
    });
    return () => {
      unlisten.then((stops) => stops.forEach((stop) => stop()));
    };
  });
</script>

<div class="batch">
  <header>
    <button type="button" onclick={() => call('batch_pick_files')}>{t('drop.batch.add_files')}</button>
    <button type="button" onclick={() => call('batch_pick_folder')}>{t('drop.batch.add_folder')}</button>
    {#if bar.pause}
      <button type="button" onclick={() => call(bar.pause === 'pause' ? 'batch_pause' : 'batch_resume')}>
        {t(bar.pause === 'pause' ? 'drop.batch.pause' : 'drop.batch.resume')}
      </button>
    {/if}
    <button type="button" disabled={!bar.cancel} onclick={() => call('batch_cancel_all')}>{t('drop.batch.cancel_all')}</button>
    <button type="button" disabled={!bar.rerun} onclick={() => call('batch_rerun', { adjust })}>{t('drop.batch.rerun')}</button>
    <button type="button" disabled={!bar.rollback} onclick={() => call('batch_rollback')}>{t('drop.batch.rollback')}</button>
    <button type="button" disabled={!bar.reveal} onclick={() => call('batch_reveal')}>{t('drop.batch.reveal')}</button>
    <span class="grow"></span>
    <button type="button" aria-pressed={adjusting} onclick={() => (adjusting = !adjusting)}>{t('drop.batch.adjust')}</button>
  </header>

  <div class="main">
    {#if rows.length === 0}
      <div class="empty">
        <h2>{t('drop.batch.empty')}</h2>
        <p>{t('drop.batch.empty.help', { count: threshold })}</p>
      </div>
    {:else}
      <BatchTable {rows} {added} bind:status={statusFilter} />
    {/if}
    {#if adjusting}<AdjustPanel {adjust} onchange={change} />{/if}
  </div>

  <footer>
    <progress value={progress} max="1" aria-hidden="true"></progress>
    <span role="status">{statusLine(status)}</span>
    {#if status.tally.failed > 0}
      <button type="button" class="failures" onclick={() => (statusFilter = 'failed')}>
        {t('drop.batch.failures', { count: status.tally.failed })}
      </button>
    {/if}
    {#if status.skipped_in_folders > 0}<small>{t('drop.batch.skipped_in_folders', { count: status.skipped_in_folders })}</small>{/if}
    {#if status.truncated}<small>{t('drop.batch.truncated', { count: 10000 })}</small>{/if}
    <span class="grow"></span>
    {#if status.phase === 'done'}
      <button type="button" onclick={() => call('batch_close')}>{t('drop.batch.close')}</button>
    {/if}
  </footer>
</div>

<style>
  .batch {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  header,
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: var(--bg-group);
  }
  header {
    border-bottom: 1px solid var(--rail-line);
    flex-wrap: wrap;
  }
  footer {
    border-top: 1px solid var(--rail-line);
  }
  .grow {
    flex: 1;
  }
  .main {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--fg-2);
  }
  button {
    font: inherit;
    color: var(--fg);
  }
  button:disabled {
    opacity: 0.45;
  }
  .failures {
    color: var(--error);
  }
  small {
    color: var(--fg-2);
  }
  progress {
    width: 160px;
  }
</style>
