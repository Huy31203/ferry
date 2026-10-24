<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '../../lib/i18n';
  import type { AppRef } from '../../lib/settings';

  let {
    value,
    disabled = false,
    labelledby,
    onchange,
    onerror,
  }: {
    value: AppRef | null;
    disabled?: boolean;
    labelledby: string;
    onchange: (value: AppRef | null) => void;
    onerror?: (why: string) => void;
  } = $props();

  async function choose() {
    try {
      const app = await invoke<AppRef | null>('share_pick_app');
      if (app) onchange(app);
    } catch (e) {
      onerror?.(t('settings.failed', { why: String(e) }));
    }
  }
</script>

<div class="picker" role="group" aria-labelledby={labelledby}>
  <span class="name">{value?.name ?? t('share.app.none')}</span>
  <button type="button" {disabled} onclick={choose}>{t('share.app.choose')}</button>
  {#if value}
    <button type="button" {disabled} onclick={() => onchange(null)}>{t('share.app.clear')}</button>
  {/if}
</div>

<style>
  .picker {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .name {
    color: var(--fg-2);
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
  button:focus-visible {
    outline: 2px solid var(--accent-text);
  }
</style>
