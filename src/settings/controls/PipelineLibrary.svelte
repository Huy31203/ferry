<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t, type StringKey } from '../../lib/i18n';
  import { duplicate, removePipeline, usage, usageLabel, type Pipelines } from '../../lib/pipelines';

  let {
    pipelines,
    disabled,
    labelledby,
    onchange,
  }: {
    pipelines: Pipelines;
    disabled: boolean;
    labelledby: string;
    onchange: (pipelines: Pipelines) => void;
  } = $props();

  /** The pipeline whose Delete button asks "Delete “X”?", for 4 s. */
  let confirming = $state<number | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const open = (pipeline: number | null, tab: string | null = null) =>
    void invoke('pipe_open_editor', { pipeline, tab });

  function askDelete(id: number) {
    if (confirming === id) {
      clearTimeout(timer);
      confirming = null;
      onchange(removePipeline(pipelines, id));
      return;
    }
    confirming = id;
    clearTimeout(timer);
    timer = setTimeout(() => (confirming = null), 4000);
  }
</script>

<div class="library">
  <ul aria-labelledby={labelledby}>
    {#each pipelines.library as pipeline (pipeline.id)}
      <li>
        <div class="text">
          <span class="name">{pipeline.name}</span>
          <span class="badge">{t(`pipe.media.${pipeline.media}` as StringKey)}</span>
          <span class="usage">{usageLabel(usage(pipelines, pipeline.id))}</span>
        </div>
        <div class="actions">
          <button type="button" onclick={() => open(pipeline.id)}>{t('pipe.edit')}</button>
          <button
            type="button"
            {disabled}
            onclick={() => onchange(duplicate(pipelines, pipeline.id, t('pipe.copy_name', { name: pipeline.name })).lib)}
            >{t('pipe.duplicate')}</button
          >
          <button type="button" class="danger" {disabled} onclick={() => askDelete(pipeline.id)}>
            {confirming === pipeline.id ? t('pipe.delete_confirm', { name: pipeline.name }) : t('pipe.delete')}
          </button>
        </div>
      </li>
    {/each}
  </ul>
  {#if pipelines.library.length === 0}
    <p class="empty">{t('pipe.empty.title')}. {t('pipe.empty.body')}</p>
  {/if}
  <div class="footer">
    <button type="button" {disabled} onclick={() => open(null, 'new')}>{t('pipe.new')}</button>
    <button type="button" onclick={() => open(null)}>{t('pipe.open_editor')}</button>
  </div>
</div>

<style>
  .library {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 0;
    border-top: 1px solid var(--rail-line);
  }
  li:first-child {
    border-top: 0;
  }
  .text {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px 8px;
    min-width: 0;
  }
  .name {
    font-weight: 600;
  }
  .badge {
    padding: 0 7px;
    font-size: 11px;
    color: var(--fg-2);
    border: 1px solid var(--rail-line);
    border-radius: 999px;
  }
  .usage,
  .empty {
    margin: 0;
    font-size: 11px;
    color: var(--fg-2);
  }
  .actions,
  .footer {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
  button {
    padding: 4px 10px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
  .danger {
    color: var(--error);
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
