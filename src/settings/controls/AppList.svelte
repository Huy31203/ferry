<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '../../lib/i18n';
  import type { AppRef } from '../../lib/settings';

  let {
    items,
    disabled = false,
    labelledby,
    onchange,
    onerror,
  }: {
    items: AppRef[];
    disabled?: boolean;
    labelledby: string;
    onchange: (items: AppRef[]) => void;
    onerror?: (why: string) => void;
  } = $props();

  async function add() {
    try {
      const app = await invoke<AppRef | null>('share_pick_app');
      if (app && !items.some((item) => item.id === app.id)) onchange([...items, app]);
    } catch (e) {
      onerror?.(t('settings.failed', { why: String(e) }));
    }
  }
</script>

<div class="list">
  <ul aria-labelledby={labelledby}>
    {#each items as item (item.id)}
      <li>
        <span>{item.name}</span>
        <button
          type="button"
          {disabled}
          aria-label={t('share.app.remove', { name: item.name })}
          onclick={() => onchange(items.filter((i) => i.id !== item.id))}>×</button
        >
      </li>
    {/each}
  </ul>
  <button type="button" class="add" {disabled} onclick={add}>{t('share.app.add')}</button>
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
    padding: 4px 10px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
  button:focus-visible {
    outline: 2px solid var(--accent-text);
  }
</style>
