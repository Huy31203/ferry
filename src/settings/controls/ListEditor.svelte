<script lang="ts">
  import { t } from '../../lib/i18n';

  let {
    items,
    mode,
    onchange,
    onpick,
    labelledby,
    disabled = false,
  }: {
    items: string[];
    /** `text` types an extension; `picker` asks `onpick` for an app. */
    mode: 'text' | 'picker';
    onchange: (items: string[]) => void;
    onpick?: () => Promise<string | null>;
    labelledby: string;
    disabled?: boolean;
  } = $props();

  let draft = $state('');

  function add(item: string) {
    const clean = item.trim().replace(/^\./, '').toLowerCase();
    if (clean && !items.includes(clean)) onchange([...items, clean]);
    draft = '';
  }

  async function pick() {
    const id = await onpick?.();
    if (id && !items.includes(id)) onchange([...items, id]);
  }
</script>

<div class="list">
  <ul aria-labelledby={labelledby}>
    {#each items as item (item)}
      <li>
        <span>{item}</span>
        <button
          type="button"
          {disabled}
          aria-label={t('settings.remove', { name: item })}
          onclick={() => onchange(items.filter((i) => i !== item))}>×</button
        >
      </li>
    {/each}
  </ul>
  {#if mode === 'text'}
    <input
      type="text"
      {disabled}
      bind:value={draft}
      placeholder={t('settings.extension_placeholder')}
      aria-label={t('settings.add_extension')}
      onkeydown={(event) => {
        if (event.key === 'Enter') add(draft);
      }}
      onblur={() => add(draft)}
    />
  {:else}
    <button type="button" class="add" {disabled} onclick={pick}>{t('settings.add_app')}</button>
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
  input {
    width: 100px;
    padding: 4px 6px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  .add {
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
</style>
