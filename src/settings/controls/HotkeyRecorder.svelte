<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { acceleratorFromEvent, hotkeyLabel, problemKey } from '../../lib/hotkeys';
  import { t } from '../../lib/i18n';
  import { isMac } from '../../lib/ipc';
  import { settingsFor } from '../../lib/schema';
  import type { HotkeyId, HotkeyStatus, Verdict } from '../../lib/svc';
  import { store } from '../store.svelte';
  import Switch from './Switch.svelte';

  let {
    action,
    value,
    disabled,
    labelledby,
    label,
  }: {
    action: HotkeyId;
    value: string | undefined;
    disabled: boolean;
    labelledby: string;
    label: string;
  } = $props();

  let statuses = $state<HotkeyStatus[]>([]);
  let verdict = $state<Verdict | null>(null);
  let recording = $state(false);
  let live = $state('');
  let failure = $state<string | null>(null);

  const enabled = $derived(!store.value?.svc.shortcuts.disabled.includes(action));
  const mine = $derived(statuses.find((s) => s.id === action));

  /** The one status line: the last answer, else this key's own state. */
  const message = $derived.by(() => {
    if (failure) return failure;
    const problem = verdict ? verdict.problem : (mine?.problem ?? null);
    const key = problemKey(problem, verdict?.warning ?? null, isMac);
    if (key === null) {
      return !verdict && mine?.state === 'failed' ? t('svc.hotkey.failed') : null;
    }
    let name = '';
    if (problem?.kind === 'duplicate') {
      const other = settingsFor(isMac).find(
        (s) => s.control.kind === 'hotkey' && s.control.action === problem.with,
      );
      name = other ? t(other.label) : problem.with;
    }
    return t(key, { name });
  });

  async function loadStatuses() {
    try {
      statuses = await invoke<HotkeyStatus[]>('svc_hotkeys_status');
    } catch (e) {
      failure = String(e);
    }
  }

  onMount(() => {
    void loadStatuses();
    const unlisten = listen('svc:hotkeys', () => void loadStatuses());
    return () => void unlisten.then((off) => off());
  });

  /** Run a command that answers a verdict. */
  async function ask(command: string, args: Record<string, unknown>) {
    failure = null;
    try {
      verdict = await invoke<Verdict>(command, args);
    } catch (e) {
      failure = String(e);
    }
  }

  async function start() {
    if (recording) return;
    verdict = null;
    failure = null;
    live = '';
    recording = true;
    try {
      await invoke('svc_hotkeys_suspend', { on: true });
    } catch (e) {
      failure = String(e);
    }
  }

  /** Always give Ferry's own hotkeys back, whichever way recording ends. */
  async function finish() {
    recording = false;
    live = '';
    try {
      await invoke('svc_hotkeys_suspend', { on: false });
    } catch (e) {
      failure = String(e);
    }
  }

  function cancel() {
    if (!recording) return;
    verdict = null;
    void finish();
  }

  const MODIFIER_CODES = /^(Shift|Control|Alt|Meta|OS)(Left|Right)$/;

  async function onkeydown(e: KeyboardEvent) {
    if (!recording) return;
    const bare = !(e.ctrlKey || e.altKey || e.shiftKey || e.metaKey);
    if (e.key === 'Escape') {
      e.preventDefault();
      cancel();
      return;
    }
    // A bare Tab ends recording and lets focus move on.
    if (e.key === 'Tab' && bare) {
      cancel();
      return;
    }
    e.preventDefault();
    if (MODIFIER_CODES.test(e.code)) {
      const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Cmd'];
      live = hotkeyLabel(mods.filter(Boolean).join('+'), isMac);
      return;
    }
    const accelerator = acceleratorFromEvent(e, isMac);
    if (accelerator === null) {
      verdict = { problem: { kind: 'invalid' }, warning: null };
      return;
    }
    try {
      await ask('svc_hotkey_set', { id: action, accelerator });
    } finally {
      await finish();
    }
  }
</script>

<div class="recorder">
  {#if action === 'scale'}
    <span class="keys">{t('svc.hotkey.scale_keys')}</span>
  {:else}
    <button
      type="button"
      class="chord"
      class:recording
      disabled={disabled || !enabled}
      aria-label={t('svc.hotkey.record', { name: label })}
      onclick={start}
      {onkeydown}
      onblur={cancel}
    >
      {#if recording}
        {live || t('svc.hotkey.recording')}
      {:else if value}
        {hotkeyLabel(value, isMac)}
      {:else}
        {t('svc.hotkey.none')}
      {/if}
    </button>
    <button
      type="button"
      class="reset"
      disabled={disabled || recording}
      aria-label={`${t('svc.hotkey.reset')}: ${label}`}
      onclick={() => ask('svc_hotkey_reset', { id: action })}
    >
      {t('svc.hotkey.reset')}
    </button>
  {/if}
  <Switch
    checked={enabled}
    {disabled}
    {labelledby}
    label={t('svc.hotkey.enable', { name: label })}
    onchange={(on) => ask('svc_hotkey_enable', { id: action, on })}
  />
  <div class="status" role="status" aria-live="polite">{message ?? ''}</div>
</div>

<style>
  .recorder {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }
  .chord,
  .reset {
    padding: 4px 12px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
  .chord {
    min-width: 96px;
  }
  .chord.recording {
    border-color: var(--accent);
    color: var(--accent-text);
  }
  .reset {
    font-size: 11px;
  }
  .chord:disabled,
  .reset:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .chord:focus-visible,
  .reset:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  .keys {
    font-size: 13px;
    color: var(--fg-2);
  }
  .status {
    flex-basis: 100%;
    min-height: 0;
    font-size: 11px;
    color: var(--fg-2);
    text-align: right;
  }
  .status:empty {
    display: none;
  }
</style>
