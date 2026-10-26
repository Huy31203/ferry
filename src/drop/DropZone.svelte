<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { t } from '../lib/i18n';
  import type { ZoneView } from '../lib/drop-ipc';
  import type { Settings } from '../lib/settings';
  import { presetsHint, zoneText } from '../lib/zone';

  let view = $state<ZoneView>({
    state: 'hidden',
    mode: 'zone',
    summary: null,
    keep_original: false,
    opens_batch: false,
  });
  const text = $derived(zoneText(view));
  let zones = $state(0); // preset zones in settings: the Ctrl tap opens them
  const hint = $derived(presetsHint(view, zones));

  onMount(() => {
    // Listen first, then ask what to show, so nothing falls in between.
    const unlisten = listen<ZoneView>('drop:view', (e) => (view = e.payload)).then(
      async (stop) => {
        view = await invoke<ZoneView>('zone_ready');
        return stop;
      },
    );
    const follow = listen<Settings>('settings:changed', (e) => (zones = e.payload.pipelines.zones.length));
    invoke<Settings>('settings_get')
      .then((settings) => (zones = settings.pipelines.zones.length))
      .catch((e) => console.warn('settings_get', e));
    return () => {
      unlisten.then((stop) => stop());
      follow.then((stop) => stop());
    };
  });
</script>

{#if view.mode === 'tab'}
  <div class="tab" title={t('drop.zone.tab')} aria-label={t('drop.zone.aria')}>
    <span></span>
  </div>
{:else}
  <div class="frame">
    <div
      class="zone"
      class:hover={view.state === 'hover'}
      class:dim={text.dim}
      class:dropped={view.state === 'dropped'}
      class:hidden={view.state === 'hidden'}
      aria-label={t('drop.zone.aria')}
    >
      <svg aria-hidden="true" width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v12M7 10l5 5 5-5M4 20h16" /></svg>
      <strong role="status">{text.label}</strong>
      {#if text.sub}<span>{text.sub}</span>{/if}
      {#if text.keep}<span class="keep">{text.keep}</span>{/if}
      {#if hint}<span class="hint">{hint}</span>{/if}
    </div>
  </div>
{/if}

<style>
  :global(html),
  :global(body) {
    background: transparent;
    overflow: hidden;
    user-select: none;
  }
  .frame {
    box-sizing: border-box;
    width: 100vw;
    height: 100vh;
    padding: 12px;
  }
  .zone {
    position: relative;
    box-sizing: border-box;
    width: 220px;
    height: 140px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border: 2px dashed var(--rail-line);
    border-radius: var(--card-radius);
    background: var(--rail-bg);
    color: var(--rail-fg);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
    text-align: center;
    transition:
      transform 120ms,
      opacity var(--motion-in);
  }
  .zone.hover {
    border: 2px solid var(--accent);
    transform: scale(1.04);
  }
  .zone.dim {
    opacity: 0.6;
  }
  .zone.dropped {
    transform: scale(0.6);
    opacity: 0;
    transition-duration: var(--motion-in);
  }
  .zone.hidden {
    opacity: 0;
  }
  span {
    font-size: 12px;
    color: var(--fg-2);
  }
  .hint {
    position: absolute;
    bottom: 8px;
    font-size: 11px;
    opacity: 0.8;
  }
  .keep {
    color: var(--accent-text);
  }
  .tab {
    width: 100vw;
    height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .tab span {
    width: 6px;
    height: 96px;
    border-radius: 3px;
    background: var(--accent);
  }
  @media (prefers-reduced-motion: reduce) {
    .zone {
      transition: none;
    }
    .zone.hover {
      transform: none;
    }
  }
</style>
