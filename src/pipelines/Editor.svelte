<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { t, type StringKey } from '../lib/i18n';
  import {
    addStep,
    duplicate,
    moveStep,
    newPipeline,
    removePipeline,
    removeStep,
    setParam,
    type Checked,
    type Media,
    type PipeMeta,
    type Pipeline,
    type Pipelines,
    type Step,
    type StepKindView,
  } from '../lib/pipelines';
  import { change, load, store } from '../settings/store.svelte';
  import Assignments from './Assignments.svelte';
  import Inspector from './Inspector.svelte';
  import Library from './Library.svelte';
  import StepList from './StepList.svelte';
  import TestWell from './TestWell.svelte';
  import TextMode from './TextMode.svelte';
  import ZonesTab from './ZonesTab.svelte';
  import Switch from '../settings/controls/Switch.svelte';

  type Tab = 'pipelines' | 'zones';

  let tab = $state<Tab>('pipelines');
  let selected = $state<number | null>(null);
  let kinds = $state<StepKindView[]>([]);
  let starters = $state<Pipeline[]>([]);
  let meta = $state<PipeMeta>({ media: [], last_run: {} });
  let name = $state('');
  let mode = $state<'visual' | 'text'>('visual');
  let step = $state<number | null>(null);
  let checked = $state<Checked>({ issues: [], captions: [] });

  const pipelines = $derived(store.value?.pipelines ?? null);
  const folders = $derived(store.value?.watch.folders ?? []);
  const current = $derived(pipelines?.library.find((p) => p.id === selected) ?? null);

  // The name box follows the selected pipeline.
  $effect(() => {
    name = current?.name ?? '';
  });

  // A pipeline of its own media: the kinds to add, and the kind of the open step.
  const mediaKinds = $derived(kinds.filter((k) => k.media === current?.media));
  const openStep = $derived(step !== null ? (current?.steps[step] ?? null) : null);
  const openKind = $derived(openStep ? mediaKinds.find((k) => k.id === openStep.kind) : undefined);

  // Problems and captions, 150 ms after any change.
  $effect(() => {
    const pipeline = current ? $state.snapshot(current) : null;
    if (!pipeline) {
      checked = { issues: [], captions: [] };
      return;
    }
    const timer = setTimeout(
      () => void invoke<Checked>('pipe_check', { pipeline }).then((result) => (checked = result)),
      150,
    );
    return () => clearTimeout(timer);
  });

  // Another pipeline, another open step.
  $effect(() => {
    void selected;
    step = null;
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

  function fromStarter(starter: Pipeline) {
    if (!pipelines) return;
    const next = newPipeline(pipelines, starter.media, starter.name, starter.steps);
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

  function update(patch: Partial<Pipeline>) {
    if (!pipelines || !current) return;
    commit({ ...pipelines, library: pipelines.library.map((p) => (p.id === current.id ? { ...p, ...patch } : p)) });
  }

  const setSteps = (steps: Step[]) => update({ steps });

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
      [kinds, meta, starters] = await Promise.all([
        invoke<StepKindView[]>('pipe_registry'),
        invoke<PipeMeta>('pipe_meta'),
        invoke<Pipeline[]>('pipe_starters'),
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
        {folders}
        {selected}
        {meta}
        {starters}
        onselect={(id) => (selected = id)}
        onnew={create}
        onstarter={fromStarter}
        onduplicate={copy}
        ondelete={remove}
      />
      <main class="centre">
        {#if current}
          <div class="head">
            <input
              class="name"
              type="text"
              aria-label={t('pipe.name')}
              bind:value={name}
              onblur={rename}
              onkeydown={(event) => event.key === 'Enter' && event.currentTarget.blur()}
            />
            {#if current.steps.length === 0}
              <select
                aria-label={t('pipe.media')}
                value={current.media}
                onchange={(event) => update({ media: event.currentTarget.value as Media })}
              >
                {#each meta.media as media (media)}
                  <option value={media}>{t(`pipe.media.${media}` as StringKey)}</option>
                {/each}
              </select>
            {/if}
          </div>
          <div class="bar">
            <div class="modes" role="group">
              {#each ['visual', 'text'] as const as id (id)}
                <button type="button" aria-pressed={mode === id} onclick={() => (mode = id)}>{t(`pipe.mode.${id}`)}</button>
              {/each}
            </div>
            {#if checked.issues.length > 0}
              <span class="issues" role="status">{t('pipe.issues', { n: checked.issues.length })}</span>
            {/if}
          </div>
          {#if mode === 'visual'}
            <StepList
              steps={current.steps}
              kinds={mediaKinds}
              captions={checked.captions}
              issues={checked.issues}
              selected={step}
              onselect={(i) => (step = i)}
              onmove={(from, to) => {
                setSteps(moveStep(current.steps, from, to));
                step = to;
              }}
              onremove={(i) => {
                setSteps(removeStep(current.steps, i));
                step = null;
              }}
              onadd={(kind) => {
                const steps = addStep(current.steps, kind);
                setSteps(steps);
                step = steps.length - 1; // the new step, last
              }}
            />
            <Inspector
              step={openStep}
              kind={openKind}
              onparam={(param, value) => step !== null && setSteps(setParam(current.steps, step, param, value))}
            />
          {:else}
            {#key current.id}
              <TextMode media={current.media} steps={current.steps} kinds={mediaKinds} onsteps={setSteps} />
            {/key}
          {/if}
        {:else}
          <p class="empty">{t('pipe.empty.title')}</p>
        {/if}
      </main>
      <aside class="right">
        {#if current}
          <Assignments {pipelines} {folders} pipeline={current} onchange={commit} />
          <div class="hide">
            <div>
              <div id="hide-result">{t('pipe.hide_result')}</div>
              <div class="help">{t('pipe.hide_result.help')}</div>
            </div>
            <Switch checked={current.hide_result} labelledby="hide-result" onchange={(on) => update({ hide_result: on })} />
          </div>
          <TestWell pipeline={current} blocked={checked.issues.length > 0} />
        {/if}
      </aside>
    </div>
  {:else}
    <ZonesTab {pipelines} {meta} onchange={commit} />
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
  .centre {
    padding: 16px;
    overflow-y: auto;
  }
  .right {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px;
    overflow-y: auto;
    border-left: 1px solid var(--rail-line);
  }
  .hide {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .help {
    font-size: 11px;
    color: var(--fg-2);
  }
  .head {
    display: flex;
    gap: 8px;
  }
  select {
    padding: 4px 8px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 12px 0 8px;
  }
  .modes {
    display: flex;
  }
  .modes button {
    padding: 3px 12px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    cursor: pointer;
  }
  .modes button:first-child {
    border-radius: 6px 0 0 6px;
  }
  .modes button:last-child {
    border-radius: 0 6px 6px 0;
  }
  .modes button[aria-pressed='true'] {
    color: #fff;
    background: var(--accent);
    border-color: var(--accent);
  }
  .issues {
    color: var(--error);
  }
  .centre {
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .centre :global(.inspector) {
    margin-top: 12px;
  }
  .name {
    box-sizing: border-box;
    flex: 1;
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
