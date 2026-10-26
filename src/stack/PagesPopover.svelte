<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { t } from '../lib/i18n';
  import type { EditsPatch, JobUpdate } from '../lib/ipc';
  import { checkPages, DPI_STOPS, dpiLabel, pagesForm, type PagesForm } from '../lib/pdf';
  import { loadPdfConfig, pdfConfig } from '../lib/pdfConfig.svelte';

  /** Extract a PDF's pages as PNG or JPEG images (sE pdf-formats). */
  let {
    card,
    onapply,
    onclose,
  }: {
    card: JobUpdate;
    onapply: (edits: EditsPatch) => void;
    onclose: () => void;
  } = $props();

  // The form starts from the card's edits once; later job updates don't reset what was chosen.
  let form = $state<PagesForm>(untrack(() => pagesForm(card, pdfConfig)));
  let tried = $state(false);

  onMount(() => {
    loadPdfConfig().catch((e) => console.warn('pdf_config', e));
  });

  const checked = $derived(checkPages(form, card.pages));
  const invalid = $derived(tried && checked.error !== null);

  function apply() {
    tried = true;
    if (checked.patch) onapply(checked.patch);
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
    node.focus();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="popover"
  role="dialog"
  aria-label={t('pdf.pages.title')}
  data-popover
  data-card-id={card.id}
  tabindex="-1"
  use:focusFirst
  onkeydown={keydown}
>
  <div class="row">
    <button type="button" class:on={form.format === 'png'} aria-pressed={form.format === 'png'} onclick={() => (form.format = 'png')}>PNG</button>
    <button type="button" class:on={form.format === 'jpeg'} aria-pressed={form.format === 'jpeg'} onclick={() => (form.format = 'jpeg')}>JPEG</button>
  </div>

  <div class="row">
    <button type="button" class:on={form.all} aria-pressed={form.all} onclick={() => (form.all = true)}>{t('pdf.pages.all')}</button>
    <button type="button" class:on={!form.all} aria-pressed={!form.all} onclick={() => (form.all = false)}>{t('pdf.pages.range')}</button>
    {#if !form.all}
      <input type="text" inputmode="numeric" aria-label={t('pdf.pages.from')} aria-invalid={invalid} bind:value={form.from} />
      <span>–</span>
      <input type="text" inputmode="numeric" aria-label={t('pdf.pages.to')} aria-invalid={invalid} bind:value={form.to} />
    {/if}
  </div>
  {#if invalid}
    <p class="error" role="alert">{checked.error}</p>
  {/if}

  <label class="row">
    <span class="muted">{t('pdf.pages.resolution')}</span>
    <select bind:value={form.dpi}>
      {#each DPI_STOPS as stop (stop)}
        <option value={stop}>{dpiLabel(stop)}</option>
      {/each}
      {#if !DPI_STOPS.includes(form.dpi)}
        <option value={form.dpi}>{dpiLabel(form.dpi)}</option>
      {/if}
    </select>
  </label>

  <div class="row end">
    <button type="button" class="apply" onclick={apply}>{t('pdf.pages.apply')}</button>
  </div>
</div>

<style>
  .popover {
    width: 240px;
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
  .row.end {
    justify-content: flex-end;
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
  input,
  select {
    padding: 2px 8px;
    font: inherit;
    color: #fff;
    color-scheme: dark;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.25);
    border-radius: 6px;
  }
  input {
    width: 44px;
  }
  input[aria-invalid='true'] {
    border-color: #ff453a;
  }
  .muted {
    opacity: 0.65;
  }
  .error {
    margin: 0;
    color: #ffb3ae;
  }
  .apply {
    padding: 3px 12px;
    font-weight: 600;
    background: #0a84ff;
  }
</style>
