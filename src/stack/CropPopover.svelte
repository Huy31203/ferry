<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { untrack } from 'svelte';
  import { ASPECTS, checkCrop, cropForm, cropFrame, scaleForm, type CropForm, type CropFrame } from '../lib/edits';
  import { t } from '../lib/i18n';
  import type { EditsPatch, JobUpdate } from '../lib/ipc';

  /** Crop beside the card (spec §5.3, mockup "crop popover A"). */
  let {
    card,
    sizes,
    onapply,
    onclose,
    onframe,
  }: {
    card: JobUpdate;
    /** The saved sizes from settings (`images.crop_sizes`). */
    sizes: [number, number][];
    onapply: (edits: EditsPatch) => void;
    onclose: () => void;
    /** The live crop frame for the card's thumbnail. */
    onframe: (frame: CropFrame | null) => void;
  } = $props();

  // The form starts from the card's edits once; later job updates don't reset what is typed.
  let form = $state<CropForm>(untrack(() => cropForm(card.edits)));
  const check = $derived(checkCrop(form, card.dims_before));

  $effect(() => {
    onframe(cropFrame(card.dims_before, form));
    return () => onframe(null);
  });

  function apply() {
    if (check.patch) onapply(check.patch);
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
    node.querySelector<HTMLInputElement>('input')?.focus();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="popover"
  role="dialog"
  aria-label={t('crop.title')}
  data-popover
  data-card-id={card.id}
  tabindex="-1"
  use:focusFirst
  onkeydown={keydown}
>
  <div class="seg">
    {#each ASPECTS as aspect (aspect.label)}
      <button
        type="button"
        class:on={same(form.aspect, aspect.ratio)}
        aria-pressed={same(form.aspect, aspect.ratio)}
        onclick={() => (form.aspect = aspect.ratio)}>{aspect.ratio ? aspect.label : t('crop.free')}</button
      >
    {/each}
  </div>

  <div class="fields">
    {#if form.mode === 'size'}
      <input
        type="text"
        inputmode="numeric"
        aria-label={t('crop.width')}
        aria-invalid={check.field === 'width'}
        bind:value={form.width}
      />
      <span>×</span>
      <input
        type="text"
        inputmode="numeric"
        aria-label={t('crop.height')}
        aria-invalid={check.field === 'height'}
        bind:value={form.height}
      />
      <span class="muted">{t('crop.keeps_ratio')}</span>
    {:else}
      <input
        type="text"
        inputmode="numeric"
        aria-label={t('crop.long_edge')}
        aria-invalid={check.field === 'edge'}
        bind:value={form.edge}
      />
    {/if}
  </div>

  <div class="seg">
    <button type="button" class:on={form.mode === 'size'} aria-pressed={form.mode === 'size'} onclick={() => (form.mode = 'size')}>{t('crop.size')}</button>
    <button type="button" class:on={form.mode === 'long_edge'} aria-pressed={form.mode === 'long_edge'} onclick={() => (form.mode = 'long_edge')}>{t('crop.long_edge')}</button>
    <span class="gap"></span>
    <button type="button" class:on={form.framing === 'smart'} aria-pressed={form.framing === 'smart'} onclick={() => (form.framing = 'smart')}>{t('crop.smart')}</button>
    <button type="button" class:on={form.framing === 'centre'} aria-pressed={form.framing === 'centre'} onclick={() => (form.framing = 'centre')}>{t('crop.centre')}</button>
    <span class="gap"></span>
    <button type="button" onclick={() => (form = scaleForm(form, 0.5))}>{t('crop.half')}</button>
    <button type="button" onclick={() => (form = scaleForm(form, 2))}>{t('crop.double')}</button>
  </div>

  <div class="saved">
    <span class="muted">{t('crop.saved')}:</span>
    {#each sizes as [w, h] (`${w}x${h}`)}
      <button
        type="button"
        class="link"
        onclick={() => (form = { ...form, mode: 'size', width: String(w), height: String(h) })}>{w}×{h}</button
      >
    {/each}
    <button type="button" class="link" onclick={() => invoke('crop_sizes_reset').catch((e) => console.warn('crop_sizes_reset', e))}>{t('crop.restore_defaults')}</button>
  </div>

  {#if check.error}
    <p class="error" role="alert">{check.error}</p>
  {:else if check.larger}
    <p class="muted">{t('crop.larger')}</p>
  {/if}

  <button type="button" class="apply" disabled={check.patch === null} onclick={apply}>{t('crop.apply')}</button>
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
  .seg,
  .saved {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  .gap {
    width: 6px;
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
  button.link {
    background: none;
    padding: 2px 4px;
    text-decoration: underline;
  }
  .fields {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  input {
    width: 60px;
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
  .muted {
    margin: 0;
    opacity: 0.65;
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
