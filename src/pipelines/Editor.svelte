<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { t } from '../lib/i18n';
  import {
    duplicate,
    newPipeline,
    removePipeline,
    type PipeMeta,
    type Pipelines,
    type StepKindView,
  } from '../lib/pipelines';
  import { change, load, store } from '../settings/store.svelte';
  import Library from './Library.svelte';

  type Tab = 'pipelines' | 'zones';

  let tab = $state<Tab>('pipelines');
  let selected = $state<number | null>(null);
  let kinds = $state<StepKindView[]>([]);
  let meta = $state<PipeMeta>({ media: [], last_run: {} });
  let name = $state('');

  const pipelines = $derived(store.value?.pipelines ?? null);
  const current = $derived(pipelines?.library.find((p) => p.id === selected) ?? null);

  // The name box follows the selected pipeline.
  $effect(() => {
    name = current?.name ?? '';
  });

  const commit = (next: Pipelines) => change('pipelines', next);

  /** `tab` is "pipelines", "zones", or "new" (the Settings pane's New). */
  function focus(pipeline: number | null, requested: string | null) {
    tab = requested === 'zones' ? 'zones' : 'pipelines';
    if (requested === 'new') create();
    else if (pipeline !== null) selected = pipeline;
    else if (selected === null) selected = pipelines?.library[0]?.id ?? null;
  }

  function create() {
    if (!pipelines) return;
    const next = newPipeline(pipelines, meta.media[0] ?? 'image', t('pipe.untitled'));
    commit(next.lib);
    selected = next.id;
  }

  function copy(id: number) {
    if (!pipelines) return;
    const source = pipelines.library.find((p) => p.id === id);
    if (!source) return;
    const next = duplicate(pipelines, id, t('pipe.copy_name', { name: source.name }));
    commit(next.lib);
    selected = next.id;
  }

  function remove(id: number) {
    if (!pipelines) return;
    const rest = removePipeline(pipelines, id);
    commit(rest);
    selected = rest.library[0]?.id ?? null;
  }

  /** Saved on blur or Enter; a blank name goes back to the old one. */
  function rename() {
    if (!pipelines || !current) return;
    const next = name.trim();
    if (next === '') name = current.name;
    else if (next !== current.name)
      commit({ ...pipelines, library: pipelines.library.map((p) => (p.id === current.id ? { ...p, name: next } : p)) });
  }

  onMount(() => {
    const params = new URLSearchParams(location.search);
    const wanted = params.get('pipeline');
    void (async () => {
      await load();
      [kinds, meta] = await Promise.all([
        invoke<StepKindView[]>('pipe_registry'),
        invoke<PipeMeta>('pipe_meta'),
      ]);
      focus(wanted === null ? null : Number(wanted), params.get('tab'));
    })();
    const unlisten = listen<{ pipeline: number | null; tab: string | null }>('pipe:focus', (event) =>
      focus(event.payload.pipeline, event.payload.tab),
    );
    return () => void unlisten.then((off) => off());
  });
</script>

<div class="editor">
  <div class="tabs" role="tablist">
    {#each ['pipelines', 'zones'] as const as id (id)}
      <button
        type="button"
        role="tab"
        aria-selected={tab === id}
        class:selected={tab === id}
        onclick={() => (tab = id)}>{t(`pipe.tab.${id}`)}</button
      >
    {/each}
  </div>
  {#if !pipelines}
    <p class="loading"></p>
  {:else if tab === 'pipelines'}
    <div class="shell">
      <Library
        {pipelines}
        {selected}
        {meta}
        onselect={(id) => (selected = id)}
        onnew={create}
        onduplicate={copy}
        ondelete={remove}
      />
      <main class="centre">
        {#if current}
          <input
            class="name"
            type="text"
            aria-label={t('pipe.name')}
            bind:value={name}
            onblur={rename}
            onkeydown={(event) => event.key === 'Enter' && event.currentTarget.blur()}
          />
        {:else}
          <p class="empty">{t('pipe.empty.title')}</p>
        {/if}
      </main>
      <aside class="right"></aside>
    </div>
  {:else}
    <div class="shell zones"></div>
  {/if}
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .tabs {
    display: flex;
    gap: 4px;
    padding: 8px 12px 0;
    border-bottom: 1px solid var(--rail-line);
  }
  .tabs button {
    padding: 6px 14px;
    font: inherit;
    color: var(--fg-2);
    background: none;
    border: 0;
    border-bottom: 2px solid transparent;
    cursor: pointer;
  }
  .tabs button.selected {
    color: var(--fg);
    border-bottom-color: var(--accent);
  }
  .shell {
    display: grid;
    flex: 1;
    grid-template-columns: 240px minmax(0, 1fr) 280px;
    min-height: 0;
  }
  .shell.zones {
    grid-template-columns: 1fr;
  }
  .centre {
    padding: 16px;
    overflow-y: auto;
  }
  .right {
    border-left: 1px solid var(--rail-line);
  }
  .name {
    box-sizing: border-box;
    width: 100%;
    padding: 6px 8px;
    font: inherit;
    font-size: 15px;
    font-weight: 600;
    color: var(--fg);
    background: var(--bg-group);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  .empty {
    color: var(--fg-2);
  }
  button:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
</style>
