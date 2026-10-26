<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import type { CropFrame } from '../lib/edits';
  import { t } from '../lib/i18n';
  import type { EditsPatch, JobUpdate } from '../lib/ipc';
  import { PDF_ASPECTS, pdfCropForm, pdfCropPatch, pdfFrame, type PdfCropForm } from '../lib/pdf';
  import { loadPdfConfig, pdfConfig } from '../lib/pdfConfig.svelte';

  /** Crop a PDF's pages to a ratio or a device, or show them whole again (sE pdf-formats). */
  let {
    card,
    onapply,
    onclose,
    onframe,
  }: {
    card: JobUpdate;
    onapply: (edits: EditsPatch) => void;
    onclose: () => void;
    /** The live crop frame for the card's thumbnail. */
    onframe: (frame: CropFrame | null) => void;
  } = $props();

  // The form starts from the card's edits once; later job updates don't reset what was chosen.
  let form = $state<PdfCropForm>(untrack(() => pdfCropForm(card.edits)));

  onMount(() => {
    loadPdfConfig().catch((e) => console.warn('pdf_config', e));
  });

  $effect(() => {
    onframe(pdfFrame(card.dims_before, form.aspect));
    return () => onframe(null);
  });

  function apply() {
    onapply(pdfCropPatch(form));
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

  function same(a: [number, number] | null, b: [number, number] | null): boolean {
    return a?.[0] === b?.[0] && a?.[1] === b?.[1];
  }

  function focusFirst(node: HTMLElement) {
    node.focus();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="popover"
  role="dialog"
  aria-label={t('pdf.crop.title')}
  data-popover
  data-card-id={card.id}
  tabindex="-1"
  use:focusFirst
  onkeydown={keydown}
>
  <div class="seg">
    <button type="button" class:on={form.aspect === null} aria-pressed={form.aspect === null} onclick={() => (form.aspect = null)}>{t('pdf.crop.original')}</button>
    {#each PDF_ASPECTS as aspect (aspect.label)}
      <button
        type="button"
        class:on={same(form.aspect, aspect.ratio)}
        aria-pressed={same(form.aspect, aspect.ratio)}
        onclick={() => (form.aspect = aspect.ratio)}>{aspect.label}</button
      >
    {/each}
  </div>

  {#if pdfConfig.devices.length > 0}
    <div class="seg">
      <span class="muted">{t('pdf.crop.devices')}:</span>
      {#each pdfConfig.devices as device (device.name)}
        <button
          type="button"
          class:on={same(form.aspect, [device.width, device.height])}
          aria-pressed={same(form.aspect, [device.width, device.height])}
          onclick={() => (form.aspect = [device.width, device.height])}>{device.name}</button
        >
      {/each}
    </div>
  {/if}

  <label class="uncrop">
    <input type="checkbox" role="switch" bind:checked={form.uncrop} />
    <span>{t('pdf.crop.uncrop')}</span>
  </label>
  <p class="muted">{t('pdf.crop.uncrop.help')}</p>

  <button type="button" class="apply" onclick={apply}>{t('pdf.crop.apply')}</button>
</div>

<style>
  .popover {
    width: 300px;
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
  .seg {
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
  .uncrop {
    display: flex;
    gap: 6px;
    align-items: center;
    cursor: pointer;
  }
  .muted {
    margin: 0;
    opacity: 0.65;
  }
  .apply {
    align-self: flex-end;
    padding: 3px 12px;
    font-weight: 600;
    background: #0a84ff;
  }
</style>
