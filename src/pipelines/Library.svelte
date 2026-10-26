<script lang="ts">
  import { t, type StringKey } from '../lib/i18n';
  import { usage, usageLabel, type PipeMeta, type Pipeline, type Pipelines } from '../lib/pipelines';
  import type { WatchedFolder } from '../lib/settings';

  let {
    pipelines,
    folders,
    selected,
    meta,
    starters,
    onselect,
    onnew,
    onstarter,
    onduplicate,
    ondelete,
  }: {
    pipelines: Pipelines;
    folders: WatchedFolder[];
    selected: number | null;
    meta: PipeMeta;
    starters: Pipeline[];
    onselect: (id: number) => void;
    onnew: () => void;
    onstarter: (starter: Pipeline) => void;
    onduplicate: (id: number) => void;
    ondelete: (id: number) => void;
  } = $props();

  /** The pipeline whose Delete button asks "Delete “X”?", for 4 s. */
  let confirming = $state<number | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  function askDelete(id: number) {
    clearTimeout(timer);
    if (confirming === id) {
      confirming = null;
      ondelete(id);
      return;
    }
    confirming = id;
    timer = setTimeout(() => (confirming = null), 4000);
  }

  const current = $derived(pipelines.library.find((p) => p.id === selected) ?? null);
</script>

<aside class="library">
  {#if pipelines.library.length === 0}
    <div class="empty">
      <h3>{t('pipe.empty.title')}</h3>
      <p>{t('pipe.empty.body')}</p>
      {#each starters as starter (starter.name)}
        <button type="button" onclick={() => onstarter(starter)}>{starter.name}</button>
      {/each}
    </div>
  {/if}
  <ul>
    {#each pipelines.library as pipeline (pipeline.id)}
      <li>
        <button
          type="button"
          class="item"
          class:selected={pipeline.id === selected}
          aria-current={pipeline.id === selected ? 'true' : undefined}
          onclick={() => onselect(pipeline.id)}
        >
          <span class="top">
            <span class="name">{pipeline.name}</span>
            <span class="badge">{t(`pipe.media.${pipeline.media}` as StringKey)}</span>
          </span>
          <span class="usage">{usageLabel(usage(pipelines, pipeline.id, folders))}</span>
          <span class="usage">
            {meta.last_run[pipeline.id]
              ? t('pipe.last_run', { time: meta.last_run[pipeline.id] })
              : t('pipe.never_run')}
          </span>
        </button>
      </li>
    {/each}
  </ul>
  <div class="buttons">
    <button type="button" onclick={onnew}>{t('pipe.new')}</button>
    <button type="button" disabled={!current} onclick={() => current && onduplicate(current.id)}>
      {t('pipe.duplicate')}
    </button>
    <button type="button" class="danger" disabled={!current} onclick={() => current && askDelete(current.id)}>
      {current && confirming === current.id ? t('pipe.delete_confirm', { name: current.name }) : t('pipe.delete')}
    </button>
  </div>
</aside>

<style>
  .library {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-sidebar);
    border-right: 1px solid var(--rail-line);
  }
  .empty {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px 8px 0;
  }
  .empty h3,
  .empty p {
    margin: 0;
  }
  .empty h3 {
    font-size: 13px;
  }
  .empty p {
    color: var(--fg-2);
  }
  .empty button {
    padding: 4px 10px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
  ul {
    flex: 1;
    margin: 0;
    padding: 8px;
    overflow-y: auto;
    list-style: none;
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 8px 10px;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: none;
    border: 0;
    border-radius: 8px;
    cursor: pointer;
  }
  .item.selected {
    color: #fff;
    background: var(--accent);
  }
  .item.selected .usage,
  .item.selected .badge {
    color: inherit;
    opacity: 0.85;
  }
  .top {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }
  .name {
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    font-size: 11px;
    color: var(--fg-2);
  }
  .usage {
    font-size: 11px;
    color: var(--fg-2);
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 8px;
    border-top: 1px solid var(--rail-line);
  }
  .buttons button {
    padding: 4px 10px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
  .buttons .danger {
    color: var(--error);
  }
  .buttons button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  button:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
</style>
