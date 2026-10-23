<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { addSize, parseSize, sizeLabel } from '../../lib/edits';
  import { t } from '../../lib/i18n';

  /** The crop popover's saved sizes (`images.crop_sizes`), newest first. */
  let {
    sizes,
    onchange,
    labelledby,
    disabled = false,
  }: {
    sizes: [number, number][];
    onchange: (sizes: [number, number][]) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();

  let draft = $state('');
  let invalid = $state(false);

  function add() {
    if (draft.trim() === '') return;
    const size = parseSize(draft);
    invalid = size === null;
    if (size === null) return;
    onchange(addSize(sizes, size));
    draft = '';
  }

  // Rust writes the defaults and announces them with `settings:changed`, which the store follows.
  function reset() {
    invoke('crop_sizes_reset').catch((e) => console.warn('crop_sizes_reset', e));
  }
</script>

<div class="list">
  <ul aria-labelledby={labelledby}>
    {#each sizes as size (sizeLabel(size))}
      <li>
        <span>{sizeLabel(size)}</span>
        <button
          type="button"
          {disabled}
          aria-label={t('settings.remove', { name: sizeLabel(size) })}
          onclick={() => onchange(sizes.filter((s) => s !== size))}>×</button
        >
      </li>
    {/each}
  </ul>
  <div class="add">
    <input
      type="text"
      {disabled}
      bind:value={draft}
      placeholder="1200×628"
      aria-label={t('sizes.add')}
      aria-invalid={invalid}
      oninput={() => (invalid = false)}
      onkeydown={(event) => {
        if (event.key === 'Enter') add();
      }}
      onblur={add}
    />
    <button type="button" {disabled} onclick={reset}>{t('crop.restore_defaults')}</button>
  </div>
  {#if invalid}
    <p class="bad" role="alert">{t('sizes.invalid')}</p>
  {/if}
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
    max-width: 320px;
  }
  ul {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 4px 2px 10px;
    border-radius: 999px;
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
  }
  li button {
    border: 0;
    background: none;
    color: var(--fg-2);
    font-size: 15px;
    line-height: 1;
    cursor: pointer;
  }
  .add {
    display: flex;
    gap: 6px;
  }
  input {
    width: 100px;
    padding: 4px 6px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  input[aria-invalid='true'] {
    border-color: var(--error);
  }
  .add button {
    padding: 4px 10px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
  button:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .bad {
    margin: 0;
    font-size: 11px;
    color: var(--error);
  }
</style>
