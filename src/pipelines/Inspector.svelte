<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t, type StringKey } from '../lib/i18n';
  import type { ParamView, Step, StepKindView } from '../lib/pipelines';
  import Switch from '../settings/controls/Switch.svelte';

  let {
    step,
    kind,
    onparam,
  }: {
    step: Step | null;
    kind: StepKindView | undefined;
    onparam: (name: string, value: unknown | undefined) => void;
  } = $props();

  /** A param's value: the step's own, else the kind's default. */
  const valueOf = (param: ParamView): unknown => step?.params[param.name] ?? kind?.defaults[param.name];

  const visible = $derived(
    (kind?.params ?? []).filter((param) => {
      if (!param.shown_when) return true;
      const other = kind?.params.find((p) => p.name === param.shown_when!.param);
      const current = step?.params[param.shown_when.param] ?? kind?.defaults[param.shown_when.param];
      return other !== undefined && param.shown_when.values.includes(String(current));
    }),
  );

  const idOf = (param: ParamView) => `param-${param.name}`;

  async function chooseFolder(param: ParamView) {
    try {
      const folder = await invoke<string | null>('pick_folder');
      if (folder) onparam(param.name, folder);
    } catch {
      // a cancelled or failed dialog leaves the value as it was
    }
  }

  function typed(param: ParamView, text: string) {
    onparam(param.name, text === '' ? undefined : text);
  }

  function numeric(param: ParamView, text: string) {
    const value = Number(text);
    onparam(param.name, text.trim() === '' || Number.isNaN(value) ? undefined : value);
  }
</script>

<section class="inspector">
  {#if !step || !kind}
    <p class="empty">{t('pipe.inspector.empty')}</p>
  {:else}
    <h3>{t(kind.label as StringKey)}</h3>
    {#each visible as param (param.name)}
      {@const control = param.control}
      {@const value = valueOf(param)}
      <div class="field">
        <label id={`label-${idOf(param)}`} for={idOf(param)}>{t(param.label as StringKey)}</label>
        {#if control.kind === 'number'}
          <input
            id={idOf(param)}
            type="number"
            min={control.min}
            max={control.max}
            step={control.step}
            value={typeof value === 'number' ? value : ''}
            onchange={(event) => numeric(param, event.currentTarget.value)}
          />
        {:else if control.kind === 'choice'}
          <select id={idOf(param)} value={String(value ?? '')} onchange={(event) => typed(param, event.currentTarget.value)}>
            {#if !param.required && value === undefined}
              <option value=""></option>
            {/if}
            {#each control.options as option (option.value)}
              <option value={option.value}>{t(option.label as StringKey)}</option>
            {/each}
          </select>
        {:else if control.kind === 'switch'}
          <Switch checked={value === true} labelledby={`label-${idOf(param)}`} onchange={(on) => onparam(param.name, on)} />
        {:else if control.kind === 'folder'}
          <span class="folder">
            <input id={idOf(param)} type="text" value={typeof value === 'string' ? value : ''} onchange={(event) => typed(param, event.currentTarget.value)} />
            <button type="button" onclick={() => chooseFolder(param)}>{t('pipe.test.choose')}</button>
          </span>
        {:else}
          <input id={idOf(param)} type="text" value={typeof value === 'string' ? value : ''} onchange={(event) => typed(param, event.currentTarget.value)} />
        {/if}
      </div>
    {/each}
  {/if}
</section>

<style>
  .inspector {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    background: var(--bg-group);
    border: 1px solid var(--rail-line);
    border-radius: 8px;
  }
  h3 {
    margin: 0;
    font-size: 13px;
  }
  .empty {
    margin: 0;
    color: var(--fg-2);
  }
  .field {
    display: grid;
    grid-template-columns: 120px minmax(0, 1fr);
    align-items: center;
    gap: 8px;
  }
  label {
    color: var(--fg-2);
  }
  input[type='text'],
  input[type='number'],
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
  .folder {
    display: flex;
    gap: 6px;
  }
  .folder button {
    flex-shrink: 0;
    padding: 4px 10px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
  input:focus-visible,
  select:focus-visible,
  button:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
</style>
