<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { t } from '../lib/i18n';
  import { isMac } from '../lib/ipc';
  import type { Settings } from '../lib/settings';
  import { applyChoices, fromSettings, type Choices, type Encoder } from './choices';

  const ENCODERS: Encoder[] = ['fast', 'small', 'visually_lossless'];
  let settings = $state<Settings | null>(null);
  let initial: Choices | null = null;
  let c = $state<Choices | null>(null);
  let times = $state<Record<string, string>>({});
  let saving = false;

  onMount(() => {
    void invoke<Settings>('settings_get').then((s) => {
      settings = s;
      initial = fromSettings(s);
      c = { ...initial };
      void invoke('onboarding_measure');
    });
    const offs = [
      listen<{ encoder: string; label: string }>('onboarding:measured', (e) => (times[e.payload.encoder] = e.payload.label)),
      listen('onboarding:closing', () => void done()),
    ];
    return () => offs.forEach((p) => void p.then((fn) => fn()));
  });

  // Also closes when the settings never loaded, so the window can't get stuck open.
  async function done() {
    if (saving) return;
    saving = true;
    try {
      if (settings && initial && c) {
        await invoke('settings_set', { settings: applyChoices($state.snapshot(settings), initial, $state.snapshot(c), isMac) });
      }
    } finally {
      await invoke('onboarding_closed');
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Enter' && void done()} />

{#if settings && c}
  <main>
    <header>
      <h1>{t('onboarding.title')}</h1>
      <p>{t('onboarding.intro')}</p>
    </header>

    <section>
      <h2>{t('onboarding.original')}</h2>
      <div class="cards">
        <label class="card">
          <input type="radio" name="original" value={false} bind:group={c.keepOriginal} />
          <span><strong>{t('onboarding.replace')}</strong>
            <small>{t('onboarding.replace.note', { days: String(settings.backups.retention_days) })}</small></span>
        </label>
        <label class="card">
          <input type="radio" name="original" value={true} bind:group={c.keepOriginal} />
          <span><strong>{t('onboarding.keep')}</strong><small>{t('onboarding.keep.note')}</small></span>
        </label>
      </div>
    </section>

    <section>
      <h2>{t('onboarding.videos')}</h2>
      <label class="quality">
        <span>{t('onboarding.quality')}</span>
        <input type="range" min="5" max="100" bind:value={c.quality} />
        <output>{c.quality}</output>
      </label>
      <small>{t('onboarding.quality.help')}</small>
      <h3>{t('onboarding.encoder')}</h3>
      {#each ENCODERS as e (e)}
        <label class="row">
          <input type="radio" name="encoder" value={e} bind:group={c.encoder} />
          <span class="grow"><strong>{t(`video.encoder.${e}`)}</strong><small>{t(`video.encoder.${e}.help`)}</small></span>
          <span class="time">{times[e] ?? t('onboarding.measuring')}</span>
        </label>
      {/each}
    </section>

    {#if isMac}
      <section>
        <h2>{t('onboarding.archive')}</h2>
        <label class="row">
          <input type="radio" name="archive" value="replace" bind:group={c.archive} />
          {t('floating.shelf_archive_result.replace')}
        </label>
        <label class="row">
          <input type="radio" name="archive" value="add" bind:group={c.archive} />
          {t('floating.shelf_archive_result.add')}
        </label>
      </section>
    {/if}

    <footer>
      <button type="button" class="primary" onclick={done}>{t('onboarding.done')}</button>
    </footer>
  </main>
{/if}

<style>
  :global(body) { margin: 0; background: var(--bg-content); color: var(--fg); font: 13px var(--font-ui); }
  main { display: flex; flex-direction: column; min-height: 100vh; padding: 16px; box-sizing: border-box; gap: 12px; }
  h1 { margin: 0; font-size: 18px; }
  header p { margin: 4px 0 0; color: var(--fg-2); }
  section { display: flex; flex-direction: column; gap: 6px; background: var(--bg-group); border-radius: var(--card-radius); padding: 12px; }
  h2 { margin: 0 0 4px; font-size: 13px; }
  h3 { margin: 6px 0 0; font-size: 12px; }
  .cards { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .card { display: flex; gap: 6px; align-items: flex-start; border: 1px solid var(--rail-line); border-radius: 8px; padding: 8px; }
  .card span, .grow { display: flex; flex-direction: column; gap: 2px; }
  .grow { flex: 1; }
  .row, .quality { display: flex; gap: 6px; align-items: center; }
  .row { align-items: flex-start; }
  .quality input { flex: 1; }
  small, .time { color: var(--fg-2); }
  .time { white-space: nowrap; }
  footer { display: flex; justify-content: flex-end; margin-top: auto; }
  .primary { background: var(--accent); color: #fff; border: 0; border-radius: 6px; padding: 4px 14px; }
</style>
