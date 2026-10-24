<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import type { FolderInfo, JoinMode, SyncStatus } from '../../lib/svc';
  import { joinSummary, needsJoin, syncLine } from '../../lib/sync';
  import { store } from '../store.svelte';
  import Switch from './Switch.svelte';

  let { disabled, labelledby }: { disabled: boolean; labelledby: string } = $props();

  let status = $state<SyncStatus>({ kind: 'off' });
  /** A chosen folder that already holds settings: the user picks whose win. */
  let joining = $state<{ folder: string; info: FolderInfo } | null>(null);
  let failure = $state<string | null>(null);

  const enabled = $derived(store.value?.svc.sync.enabled ?? false);
  const folder = $derived(store.value?.svc.sync.folder ?? '');
  const line = $derived(syncLine(status, Date.now()));
  const summary = $derived(joining ? joinSummary(joining.info) : null);

  onMount(() => {
    invoke<SyncStatus>('svc_sync_status')
      .then((current) => (status = current))
      .catch((e) => (failure = String(e)));
    const unlisten = listen<SyncStatus>('svc:sync', (event) => (status = event.payload));
    return () => void unlisten.then((off) => off());
  });

  async function attempt(work: () => Promise<void>) {
    failure = null;
    try {
      await work();
    } catch (e) {
      failure = String(e);
    }
  }

  async function enable(target: string, join: JoinMode) {
    await invoke('svc_sync_enable', { folder: target, join });
    joining = null;
  }

  /** Look in the folder first: other computers there mean a choice, otherwise this computer's values go in. */
  async function begin(target: string) {
    const info = await invoke<FolderInfo>('svc_sync_inspect', { folder: target });
    if (needsJoin(info)) joining = { folder: target, info };
    else await enable(target, 'use_this');
  }

  async function choose() {
    const picked = await invoke<string | null>('pick_folder');
    if (picked) await begin(picked);
  }

  function toggle(on: boolean) {
    void attempt(async () => {
      if (!on) await invoke('svc_sync_disable');
      else if (folder) await begin(folder);
      else await choose();
    });
  }
</script>

<div class="sync">
  <div class="top">
    <Switch checked={enabled} {disabled} {labelledby} onchange={toggle} />
    <span class="folder" class:none={!folder}>{folder || t('svc.sync.no_folder')}</span>
    <button type="button" {disabled} onclick={() => attempt(choose)}>{t('svc.sync.choose')}</button>
    {#if enabled}
      <button type="button" {disabled} onclick={() => invoke('svc_sync_now')}>{t('svc.sync.now')}</button>
    {/if}
  </div>
  {#if line}
    <div class="status" role="status">{t(line.key, line.vars)}</div>
  {/if}
  {#if joining && summary}
    <div class="join" role="group" aria-label={t('svc.sync.join.title')}>
      <div class="title">{t('svc.sync.join.title')}</div>
      <div class="status">{t('svc.sync.join.from', summary)}</div>
      <div class="choices">
        <button type="button" onclick={() => attempt(() => enable(joining!.folder, 'use_folder'))}>
          {t('svc.sync.join.use_folder')}
        </button>
        <button type="button" onclick={() => attempt(() => enable(joining!.folder, 'use_this'))}>
          {t('svc.sync.join.use_this')}
        </button>
        <button type="button" onclick={() => (joining = null)}>{t('svc.sync.join.cancel')}</button>
      </div>
    </div>
  {/if}
  {#if failure}
    <div class="status" role="alert">{t('settings.failed', { why: failure })}</div>
  {/if}
</div>

<style>
  .sync {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .folder {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .folder.none,
  .status {
    font-size: 11px;
    color: var(--fg-2);
  }
  .join {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    border: 1px solid var(--rail-line);
    border-radius: 8px;
  }
  .title {
    font-size: 13px;
    font-weight: 600;
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  button {
    padding: 4px 12px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  button:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
</style>
