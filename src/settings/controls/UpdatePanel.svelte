<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { t, type StringKey } from '../../lib/i18n';
  import type { UpdateView } from '../../lib/svc';
  import { updateText, type UpdateButton } from '../../lib/updates';

  let view = $state<UpdateView | null>(null);
  let failure = $state<string | null>(null);

  const text = $derived(view ? updateText(view) : null);
  const phase = $derived(view?.state);

  const LABELS: Record<UpdateButton, StringKey> = {
    check: 'svc.updates.check_now',
    install: 'svc.updates.install',
    later: 'svc.updates.later',
    skip: 'svc.updates.skip',
    retry: 'svc.updates.retry',
    release: 'svc.updates.view_release',
  };

  onMount(() => {
    invoke<UpdateView>('svc_update_state')
      .then((current) => (view = current))
      .catch((e) => (failure = String(e)));
    const unlisten = listen<UpdateView>('svc:update', (event) => (view = event.payload));
    return () => void unlisten.then((off) => off());
  });

  /** A failed check, download or install shows in the state itself (`quiet`); other errors show here. */
  async function run(command: string, quiet = false) {
    failure = null;
    try {
      await invoke(command);
    } catch (e) {
      if (!quiet) failure = String(e);
    }
  }

  function press(button: UpdateButton) {
    switch (button) {
      case 'check':
        return run('svc_update_check', true);
      case 'install':
        return run('svc_update_install', true);
      case 'later':
        return run('svc_update_later');
      case 'skip':
        return run('svc_update_skip');
      case 'release':
        return run('svc_open_release');
      case 'retry':
        return run(
          phase?.kind === 'failed' && phase.phase === 'check' ? 'svc_update_check' : 'svc_update_install',
          true,
        );
    }
  }
</script>

<div class="panel">
  {#if view && text}
    <div class="current">{t('svc.updates.current', { version: view.current })}</div>
    <div class="line" role="status">{t(text.line, text.vars)}</div>
    {#if phase?.kind === 'downloading' && phase.total}
      <progress
        max={phase.total}
        value={phase.received}
        aria-label={t(text.line, text.vars)}
      ></progress>
    {/if}
    {#if phase?.kind === 'ready' && phase.busy > 0}
      <div class="note">{t('svc.updates.ready_busy', { count: phase.busy })}</div>
    {/if}
    {#if phase?.kind === 'available' && view.skipped}
      <div class="note">{t('svc.updates.skipped')}</div>
    {/if}
    {#if text.notes !== null}
      <div class="notes-title">{t('svc.updates.notes')}</div>
      <!-- A scrollable region must be reachable by keyboard. -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div class="notes" tabindex="0" role="region" aria-label={t('svc.updates.notes')}>{text.notes}</div>
    {/if}
    {#if text.buttons.length > 0}
      <div class="buttons">
        {#each text.buttons as button (button)}
          <button type="button" onclick={() => press(button)}>{t(LABELS[button])}</button>
        {/each}
      </div>
    {/if}
  {/if}
  {#if failure}
    <div class="note" role="alert">{t('settings.failed', { why: failure })}</div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }
  .current {
    font-size: 13px;
    font-weight: 600;
  }
  .line {
    font-size: 13px;
  }
  .note,
  .notes-title {
    font-size: 11px;
    color: var(--fg-2);
  }
  progress {
    width: 100%;
  }
  /* Release notes are plain text, never markup. */
  .notes {
    box-sizing: border-box;
    width: 100%;
    max-height: 180px;
    padding: 8px 10px;
    overflow-y: auto;
    font-size: 12px;
    white-space: pre-wrap;
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  .notes:focus-visible,
  .buttons button:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .buttons button {
    padding: 4px 12px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
</style>
