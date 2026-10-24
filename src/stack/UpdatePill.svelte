<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '../lib/i18n';
  import type { UpdateView } from '../lib/svc';

  let { view, hotAction = null }: { view: UpdateView; hotAction?: string | null } = $props();

  const version = $derived('version' in view.state ? view.state.version : '');
</script>

<button
  type="button"
  class="pill"
  class:hot={hotAction === 'update-pill'}
  data-action="update-pill"
  aria-label={t('svc.pill.aria', { version })}
  onclick={() => invoke('svc_open_about').catch((e) => console.warn('svc_open_about', e))}
>
  <svg aria-hidden="true" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5v14" /><path d="m19 12-7 7-7-7" /></svg>
  {t('svc.pill.label')}
</button>

<style>
  .pill {
    align-self: center;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border: 0;
    border-radius: 999px;
    background: var(--rail-bg);
    color: var(--rail-fg);
    font: inherit;
    font-size: 12px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2), 0 0 0 0.5px var(--rail-line);
    cursor: pointer;
  }
  .pill:hover,
  .pill.hot {
    background: var(--rail-line);
  }
  .pill:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
</style>
