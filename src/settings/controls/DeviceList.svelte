<script lang="ts">
  import { t } from '../../lib/i18n';
  import { DEFAULT_DEVICES, deviceLabel, parseDevice } from '../../lib/pdf';
  import type { Device } from '../../lib/settings';

  /** The PDF crop popover's devices (`pdf.devices`), as name and pixel size (sE pdf-formats). */
  let {
    devices,
    onchange,
    labelledby,
    disabled = false,
  }: {
    devices: Device[];
    onchange: (devices: Device[]) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();

  let draft = $state('');
  let invalid = $state(false);

  function add() {
    if (draft.trim() === '') return;
    const device = parseDevice(draft);
    invalid = device === null;
    if (device === null) return;
    onchange([...devices.filter((d) => deviceLabel(d) !== deviceLabel(device)), device]);
    draft = '';
  }
</script>

<div class="list">
  <ul aria-labelledby={labelledby}>
    {#each devices as device (deviceLabel(device))}
      <li>
        <span>{deviceLabel(device)}</span>
        <button
          type="button"
          {disabled}
          aria-label={t('settings.remove', { name: deviceLabel(device) })}
          onclick={() => onchange(devices.filter((d) => d !== device))}>×</button
        >
      </li>
    {/each}
  </ul>
  <div class="add">
    <input
      type="text"
      {disabled}
      bind:value={draft}
      placeholder="iPad 1640×2360"
      aria-label={t('pdf.settings.devices')}
      aria-invalid={invalid}
      oninput={() => (invalid = false)}
      onkeydown={(event) => {
        if (event.key === 'Enter') add();
      }}
      onblur={add}
    />
    <button type="button" {disabled} onclick={() => onchange(DEFAULT_DEVICES.map((d) => ({ ...d })))}>{t('pdf.settings.devices.reset')}</button>
  </div>
  {#if invalid}
    <p class="bad" role="alert">{t('pdf.settings.devices.invalid')}</p>
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
    width: 150px;
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
