<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t, type StringKey } from '../../lib/i18n';
  import type { OutputLocation, OutputRule } from '../../lib/settings';
  import Choice from './Choice.svelte';

  /** One column of the File handling pane's Images row (spec §5.6). */
  let {
    value,
    converted,
    media,
    onchange,
    labelledby,
    disabled = false,
  }: {
    value: OutputRule;
    /** The Converted rule (its example is a WebP) rather than Optimised. */
    converted: boolean;
    /** The Videos row: its example is a clip rather than a screenshot. */
    media?: 'video';
    onchange: (value: OutputRule) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();

  const LOCATIONS: { value: OutputLocation; label: StringKey }[] = [
    { value: 'in_place', label: 'output.in_place' },
    { value: 'same_folder', label: 'output.same_folder' },
    { value: 'folder', label: 'output.folder' },
    { value: 'temporary', label: 'output.temporary' },
  ];
  const TOKENS = ['%f', '%e', '%z', '%y', '%m', '%d', '%P'];

  let draft = $state('');
  let example = $state('');
  let unknown = $state<string[]>([]);
  let failed = $state<string | null>(null);
  const named = $derived(value.location === 'same_folder' || value.location === 'folder');

  $effect(() => {
    draft = value.template;
  });

  // Rust resolves the example, so there is one output implementation.
  $effect(() => {
    const rule = { ...value, template: draft };
    let current = true;
    invoke<string>('output_preview', { rule, converted, media })
      .then((ok) => {
        if (!current) return;
        example = ok;
        unknown = [];
      })
      .catch((bad) => {
        if (current) unknown = bad as string[];
      });
    return () => {
      current = false;
    };
  });

  // Spread keeps fields a newer Ferry wrote.
  function set(patch: Partial<OutputRule>) {
    onchange({ ...value, ...patch });
  }

  /** Only a valid template is saved; an invalid one stays in the field with its error. */
  function commit() {
    if (unknown.length === 0 && draft !== value.template) set({ template: draft });
  }

  async function pick() {
    failed = null;
    try {
      const folder = await invoke<string | null>('pick_folder');
      if (folder) set({ folder });
    } catch (e) {
      failed = String(e);
    }
  }
</script>

<div class="rule">
  <Choice
    value={value.location}
    options={LOCATIONS.map((o) => ({ value: o.value, label: t(o.label) }))}
    {disabled}
    {labelledby}
    onchange={(location) => set({ location: location as OutputLocation })}
  />
  {#if value.location === 'folder'}
    <div class="folder">
      <span class="path">{value.folder}</span>
      <button type="button" {disabled} onclick={pick}>{t('output.choose')}</button>
    </div>
  {/if}
  {#if named}
    <input
      type="text"
      {disabled}
      bind:value={draft}
      aria-labelledby={labelledby}
      aria-invalid={unknown.length > 0}
      onchange={commit}
      onkeydown={(event) => {
        if (event.key === 'Enter') commit();
      }}
    />
    <div class="tokens">
      {#each TOKENS as token (token)}
        <button
          type="button"
          {disabled}
          aria-label={t('output.insert', { token })}
          onclick={() => {
            draft += token;
            commit();
          }}>{token}</button
        >
      {/each}
    </div>
  {/if}
  {#if unknown.length > 0}
    <p class="bad" role="alert">{t('settings.unknown_token', { token: unknown[0] })}</p>
  {:else}
    <p class="example">{t('settings.template_example', { example })}</p>
  {/if}
  {#if failed}
    <p class="bad" role="alert">{t('settings.failed', { why: failed })}</p>
  {/if}
</div>

<style>
  .rule {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 300px;
  }
  .folder,
  .tokens {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
    font-size: 12px;
    color: var(--fg-2);
  }
  input {
    box-sizing: border-box;
    width: 100%;
    padding: 4px 8px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  input[aria-invalid='true'] {
    border-color: var(--error);
  }
  button {
    padding: 2px 8px;
    font: inherit;
    font-size: 12px;
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
  input:focus-visible,
  button:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  p {
    margin: 0;
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  .example {
    color: var(--fg-2);
  }
  .bad {
    color: var(--error);
  }
</style>
