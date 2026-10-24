<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { formatBytes } from '../lib/format';
  import { t, type StringKey } from '../lib/i18n';
  import type { Setting } from '../lib/schema';
  import type { OutputRule as Rule } from '../lib/settings';
  import Choice from './controls/Choice.svelte';
  import HotkeyRecorder from './controls/HotkeyRecorder.svelte';
  import ListEditor from './controls/ListEditor.svelte';
  import NumberField from './controls/NumberField.svelte';
  import OutputRule from './controls/OutputRule.svelte';
  import RailEditor from './controls/RailEditor.svelte';
  import SizeList from './controls/SizeList.svelte';
  import Slider from './controls/Slider.svelte';
  import Switch from './controls/Switch.svelte';
  import SyncControl from './controls/SyncControl.svelte';
  import TemplateField from './controls/TemplateField.svelte';
  import TrayStyle from './controls/TrayStyle.svelte';
  import UpdatePanel from './controls/UpdatePanel.svelte';

  let {
    setting,
    value,
    disabled,
    hint,
    highlight,
    onchange,
  }: {
    setting: Setting;
    value: unknown;
    disabled: boolean;
    /** "Turn on '…' first." when a switch this row depends on is off. */
    hint: string | null;
    highlight: boolean;
    onchange: (value: unknown) => void;
  } = $props();

  const labelId = $derived(`label-${setting.id}`);
  const control = $derived(setting.control);
  let outcome = $state<string | null>(null);
  let running = $state(false);

  async function run(action: 'open_backups' | 'open_work' | 'force_clean') {
    if (running) return;
    outcome = null;
    running = true;
    try {
      if (action === 'force_clean') {
        const done = await invoke<{ files: number; bytes: number; failed: number }>('force_clean');
        const size = formatBytes(done.bytes);
        outcome =
          done.failed > 0
            ? t('settings.cleaned_partial', { files: done.files, size, failed: done.failed })
            : t('settings.cleaned', { files: done.files, size });
      } else {
        await invoke(action);
      }
    } catch (e) {
      outcome = t('settings.failed', { why: String(e) });
    } finally {
      running = false;
    }
  }

  async function pickApp(): Promise<string | null> {
    try {
      return await invoke<string | null>('pick_app');
    } catch (e) {
      outcome = t('settings.failed', { why: String(e) });
      return null;
    }
  }
</script>

<div class="row" class:highlight class:wide={setting.wide} id={`row-${setting.id}`}>
  <div class="text">
    <div class="label" id={labelId}>{t(setting.label)}</div>
    {#if hint}
      <div class="help">{hint}</div>
    {:else if setting.help}
      <div class="help">{t(setting.help)}</div>
    {/if}
    {#if outcome}
      <div class="help" role="status">{outcome}</div>
    {/if}
  </div>
  <div class="control">
    {#if control.kind === 'switch'}
      <Switch checked={value as boolean} {disabled} labelledby={labelId} {onchange} />
    {:else if control.kind === 'slider'}
      <Slider
        value={value as number}
        min={control.min}
        max={control.max}
        {disabled}
        labelledby={labelId}
        {onchange}
      />
    {:else if control.kind === 'number'}
      <NumberField
        value={value as number}
        min={control.min}
        max={control.max}
        unit={t(control.unit)}
        {disabled}
        labelledby={labelId}
        {onchange}
      />
    {:else if control.kind === 'select'}
      <Choice
        value={value as string}
        options={control.options.map((o) => ({ value: o.value, label: t(o.label) }))}
        {disabled}
        labelledby={labelId}
        {onchange}
      />
    {:else if control.kind === 'template'}
      <TemplateField value={value as string} {disabled} labelledby={labelId} {onchange} />
    {:else if control.kind === 'extensions'}
      <ListEditor
        items={value as string[]}
        mode="text"
        {disabled}
        labelledby={labelId}
        onchange={onchange}
      />
    {:else if control.kind === 'apps'}
      <ListEditor
        items={value as string[]}
        mode="picker"
        onpick={pickApp}
        {disabled}
        labelledby={labelId}
        onchange={onchange}
      />
    {:else if control.kind === 'output'}
      <OutputRule
        value={value as Rule}
        converted={control.converted}
        {disabled}
        labelledby={labelId}
        {onchange}
      />
    {:else if control.kind === 'sizes'}
      <SizeList sizes={value as [number, number][]} {disabled} labelledby={labelId} {onchange} />
    {:else if control.kind === 'tray_style'}
      <TrayStyle value={value as string} labelledby={labelId} {onchange} />
    {:else if control.kind === 'hotkey'}
      <HotkeyRecorder
        action={control.action}
        value={value as string | undefined}
        {disabled}
        labelledby={labelId}
        label={t(setting.label)}
      />
    {:else if control.kind === 'rail'}
      <RailEditor value={value as string[]} {disabled} labelledby={labelId} {onchange} />
    {:else if control.kind === 'sync'}
      <SyncControl {disabled} labelledby={labelId} />
    {:else if control.kind === 'update'}
      <UpdatePanel />
    {:else if control.kind === 'action'}
      <button
        type="button"
        class="action"
        disabled={running}
        aria-labelledby={labelId}
        onclick={() => run(control.action)}
      >
        {t(control.button as StringKey)}
      </button>
    {/if}
  </div>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--rail-line);
    transition: background 300ms;
  }
  .row:last-child {
    border-bottom: 0;
  }
  .row.wide {
    flex-direction: column;
    align-items: stretch;
  }
  .wide .control {
    flex-shrink: 1;
  }
  .row.highlight {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .text {
    min-width: 0;
  }
  .label {
    font-size: 13px;
  }
  .help {
    margin-top: 2px;
    font-size: 11px;
    color: var(--fg-2);
  }
  .control {
    flex-shrink: 0;
  }
  .action {
    padding: 4px 12px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
  .action:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .action:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  @media (prefers-reduced-motion: reduce) {
    .row {
      transition: none;
    }
  }
</style>
