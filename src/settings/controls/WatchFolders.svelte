<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import type { WatchedFolder } from '../../lib/settings';
  import {
    MEDIA,
    addFolder,
    folderName,
    formatIgnore,
    mediaLabel,
    parseIgnore,
    pipelineId,
    removeFolder,
    statusFor,
    statusText,
    statusTone,
    updateFolder,
  } from '../../lib/watch';
  import { store } from '../store.svelte';
  import { watch } from '../watchStore.svelte';
  import NumberField from './NumberField.svelte';
  import Switch from './Switch.svelte';

  let {
    folders,
    disabled,
    labelledby,
    onchange,
  }: {
    folders: WatchedFolder[];
    disabled: boolean;
    labelledby: string;
    onchange: (folders: WatchedFolder[]) => void;
  } = $props();

  let message = $state<string | null>(null);
  const library = $derived(store.value?.pipelines.library ?? []);

  function add(path: string) {
    const next = addFolder(folders, path);
    if (next.duplicate) message = t('watch.error.duplicate');
    else onchange(next.folders);
  }

  async function pick() {
    message = null;
    try {
      const path = await invoke<string | null>('watch_pick_folder');
      if (path) add(path);
    } catch (e) {
      message = String(e);
    }
  }

  async function dropped(paths: string[]) {
    message = null;
    for (const path of paths) {
      try {
        add(await invoke<string>('watch_check_folder', { path }));
      } catch (e) {
        message = String(e);
      }
    }
  }

  function patch(index: number, change: Partial<WatchedFolder>) {
    onchange(updateFolder(folders, index, change));
  }

  function commitIgnore(index: number, input: HTMLInputElement) {
    const ignore = parseIgnore(input.value);
    input.value = formatIgnore(ignore);
    patch(index, { ignore });
  }

  // Dropping a folder on the pane adds it. The pane exists only while it is open.
  onMount(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === 'drop') void dropped(event.payload.paths);
    });
    return () => void unlisten.then((off) => off());
  });
</script>

<div class="folders">
  <table aria-labelledby={labelledby}>
    <thead>
      <tr>
        <th class="folder">{t('watch.col.folder')}</th>
        <th class="types">{t('watch.col.types')}</th>
        <th class="card">{t('watch.col.card')}</th>
        <th class="pipeline">{t('watch.col.pipeline')}</th>
        <th class="ignore">{t('watch.col.ignore')}</th>
        <th class="atonce">{t('watch.col.at_once')}</th>
        <th class="actions"></th>
      </tr>
    </thead>
    <tbody>
      {#each folders as folder, index (folder.path)}
        {@const name = folderName(folder.path)}
        {@const state = statusFor(watch.status, folder.path)}
        <tr>
          <td class="folder">
            <div class="path" title={folder.path}>{folder.path}</div>
            {#if state}
              <span class="chip {statusTone(state)}">{statusText(state, false)}</span>
            {/if}
          </td>
          <td class="types">
            {#each MEDIA as media (media)}
              {@const unsupported = watch.support !== null && !watch.support[media]}
              <button
                type="button"
                class="type"
                aria-pressed={folder.types[media]}
                disabled={disabled || unsupported}
                title={unsupported ? t('watch.unsupported') : undefined}
                onclick={() => patch(index, { types: { ...folder.types, [media]: !folder.types[media] } })}
                >{t(mediaLabel(media))}</button
              >
            {/each}
          </td>
          <td class="card">
            <Switch
              checked={folder.show_results}
              {disabled}
              labelledby={labelledby}
              label={t('watch.card.aria', { folder: name })}
              onchange={(on) => patch(index, { show_results: on })}
            />
          </td>
          <td class="pipeline">
            <select
              value={folder.pipeline ?? ''}
              {disabled}
              aria-label={t('watch.pipeline.aria', { folder: name })}
              onchange={(event) => patch(index, { pipeline: pipelineId(event.currentTarget.value) })}
            >
              <option value="">{t('watch.pipeline.none')}</option>
              {#each library as pipeline (pipeline.id)}
                <option value={pipeline.id}>{pipeline.name}</option>
              {/each}
            </select>
          </td>
          <td class="ignore">
            <input
              type="text"
              value={formatIgnore(folder.ignore)}
              {disabled}
              placeholder={t('watch.ignore.placeholder')}
              aria-label={t('watch.ignore.aria', { folder: name })}
              onchange={(event) => commitIgnore(index, event.currentTarget)}
            />
          </td>
          <td class="atonce">
            <span class="sr" id={`at-once-${index}`}>{t('watch.at_once.aria', { folder: name })}</span>
            <NumberField
              value={folder.max_files_at_once}
              min={1}
              max={1000}
              unit=""
              {disabled}
              labelledby={`at-once-${index}`}
              onchange={(count) => patch(index, { max_files_at_once: count })}
            />
          </td>
          <td class="actions">
            <button type="button" class="text" {disabled} onclick={() => patch(index, { paused: !folder.paused })}>
              {t(folder.paused ? 'watch.resume' : 'watch.pause')}
            </button>
            <button
              type="button"
              class="remove"
              {disabled}
              aria-label={t('watch.remove', { folder: name })}
              onclick={() => onchange(removeFolder(folders, index))}>×</button
            >
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
  {#if folders.length === 0}
    <p class="empty">{t('watch.empty')}</p>
  {/if}
  <div class="add">
    <button type="button" class="text" {disabled} onclick={pick}>{t('watch.add')}</button>
    <span class="hint">{t('watch.drop_hint')}</span>
  </div>
  {#if message}
    <p class="message" role="alert">{message}</p>
  {/if}
</div>

<style>
  .folders {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  table {
    width: 100%;
    table-layout: fixed;
    border-collapse: collapse;
  }
  th {
    padding: 0 4px 4px;
    font-size: 11px;
    font-weight: 600;
    text-align: left;
    color: var(--fg-2);
  }
  td {
    padding: 6px 4px;
    vertical-align: top;
    border-top: 1px solid var(--rail-line);
  }
  .folder {
    width: 22%;
  }
  .types {
    width: 26%;
  }
  .pipeline {
    width: 16%;
  }
  .card {
    width: 56px;
  }
  .ignore {
    width: 14%;
  }
  .atonce {
    width: 80px;
  }
  .actions {
    width: 76px;
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip {
    display: inline-block;
    margin-top: 2px;
    padding: 1px 7px;
    font-size: 11px;
    border: 1px solid currentColor;
    border-radius: 999px;
  }
  .ok {
    color: #137a3d;
  }
  .warn {
    color: #8a5a00;
  }
  .off {
    color: var(--fg-2);
  }
  .error {
    color: var(--error);
  }
  @media (prefers-color-scheme: dark) {
    .ok {
      color: #6fd39a;
    }
    .warn {
      color: #f0c060;
    }
  }
  .type {
    margin: 0 4px 4px 0;
    padding: 2px 8px;
    font: inherit;
    font-size: 11px;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 999px;
    cursor: pointer;
  }
  .type[aria-pressed='true'] {
    color: #fff;
    background: var(--accent);
    border-color: var(--accent);
  }
  .type:disabled {
    opacity: 0.45;
    cursor: default;
  }
  select {
    box-sizing: border-box;
    width: 100%;
    min-width: 0;
    padding: 4px 6px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  input[type='text'] {
    box-sizing: border-box;
    width: 100%;
    min-width: 0;
    padding: 4px 6px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  .atonce :global(input) {
    width: 56px;
  }
  .text {
    padding: 4px 10px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
  .remove {
    padding: 0 6px;
    font-size: 15px;
    color: var(--fg-2);
    background: none;
    border: 0;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  button:focus-visible,
  select:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  .empty,
  .hint {
    margin: 0;
    color: var(--fg-2);
  }
  .add {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .message {
    margin: 0;
    color: var(--error);
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
