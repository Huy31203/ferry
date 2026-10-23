<script lang="ts">
  import { untrack } from 'svelte';
  import { FIT_PRESETS_MB, fitBytes, fitForm } from '../lib/edits';
  import { formatBytes } from '../lib/format';
  import { t } from '../lib/i18n';
  import type { EditsPatch, JobUpdate } from '../lib/ipc';

  /** Fit under size beside the card (spec §5.3). */
  let {
    card,
    onapply,
    onclose,
  }: {
    card: JobUpdate;
    onapply: (edits: EditsPatch) => void;
    onclose: () => void;
  } = $props();

  const start = untrack(() => fitForm(card.edits.fit_under));
  let text = $state(start.text);
  let unit = $state<'kb' | 'mb'>(start.unit);
  const bytes = $derived(fitBytes(text, unit));

  function apply(target: number | null = bytes) {
    if (target !== null) onapply({ fit_under: target });
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      apply();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      onclose();
    }
  }

  function focusFirst(node: HTMLElement) {
    node.querySelector<HTMLInputElement>('input')?.focus();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="popover"
  role="dialog"
  aria-label={t('fit.title')}
  data-popover
  data-card-id={card.id}
  tabindex="-1"
  use:focusFirst
  onkeydown={keydown}
>
  <div class="row">
    <input type="text" inputmode="decimal" aria-label={t('fit.size')} aria-invalid={text !== '' && bytes === null} bind:value={text} />
    <button type="button" class:on={unit === 'kb'} aria-pressed={unit === 'kb'} onclick={() => (unit = 'kb')}>{t('unit.kb')}</button>
    <button type="button" class:on={unit === 'mb'} aria-pressed={unit === 'mb'} onclick={() => (unit = 'mb')}>{t('unit.mb')}</button>
  </div>
  <div class="row">
    {#each FIT_PRESETS_MB as mb (mb)}
      <button type="button" onclick={() => apply(mb * 1_000_000)}>{formatBytes(mb * 1_000_000)}</button>
    {/each}
  </div>
  {#if text !== '' && bytes === null}
    <p class="error" role="alert">{t('fit.invalid')}</p>
  {/if}
  <button type="button" class="apply" disabled={bytes === null} onclick={() => apply()}>{t('fit.apply')}</button>
</div>

<style>
  .popover {
    width: 220px;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: 14px;
    background: #2c2c2e;
    color: #fff;
    font-size: 12px;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.25);
    outline: none;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  button {
    border: 0;
    border-radius: 7px;
    padding: 2px 8px;
    font: inherit;
    color: #fff;
    background: rgba(255, 255, 255, 0.12);
    cursor: pointer;
  }
  button.on {
    background: #0a84ff;
  }
  input {
    width: 70px;
    padding: 2px 8px;
    font: inherit;
    color: #fff;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.25);
    border-radius: 6px;
  }
  input[aria-invalid='true'] {
    border-color: #ff453a;
  }
  .error {
    margin: 0;
    color: #ffb3ae;
  }
  .apply {
    align-self: flex-end;
    padding: 3px 12px;
    font-weight: 600;
    background: #0a84ff;
  }
  .apply:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
