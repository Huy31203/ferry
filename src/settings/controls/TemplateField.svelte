<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '../../lib/i18n';

  let {
    value,
    onchange,
    labelledby,
    disabled = false,
  }: {
    value: string;
    onchange: (value: string) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();

  let draft = $state('');
  let example = $state('');
  let unknown = $state<string[]>([]);

  $effect(() => {
    draft = value;
  });

  // Rust renders the example, so there is one template implementation.
  $effect(() => {
    const text = draft;
    let current = true;
    invoke<string>('template_preview', { template: text })
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

  /** Only a valid template is saved; an invalid one stays in the field with its error. */
  function commit() {
    if (unknown.length === 0 && draft !== value) onchange(draft);
  }
</script>

<div class="template">
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
  {#if unknown.length > 0}
    <p class="bad" role="alert">{t('settings.unknown_token', { token: unknown[0] })}</p>
  {:else}
    <p class="example">{t('settings.template_example', { example })}</p>
  {/if}
</div>

<style>
  .template {
    width: 300px;
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
  input:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  p {
    margin: 4px 0 0;
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
