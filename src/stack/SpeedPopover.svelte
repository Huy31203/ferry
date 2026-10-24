<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '../lib/i18n';
  import type { EditsPatch, JobUpdate } from '../lib/ipc';
  import { SPEED_CHIPS, parseSpeed, speedLabel, speedPatch } from '../lib/video';

  /** Playback speed beside a video card. */
  let {
    card,
    framesDefault,
    onapply,
    onclose,
  }: {
    card: JobUpdate;
    /** The Videos setting's starting toggle: "keep" or "drop". */
    framesDefault: 'keep' | 'drop';
    onapply: (edits: EditsPatch) => void;
    onclose: () => void;
  } = $props();

  const current = untrack(() => card.edits.speed);
  let text = $state('');
  let keepFrames = $state(untrack(() => current?.keep_frames ?? framesDefault === 'keep'));
  const typed = $derived(parseSpeed(text));

  function apply(percent: number | null) {
    onapply(speedPatch(percent, keepFrames));
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      if (typed !== null) apply(typed);
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
  aria-label={t('video.speed.title')}
  data-popover
  data-card-id={card.id}
  tabindex="-1"
  use:focusFirst
  onkeydown={keydown}
>
  <div class="row">
    {#each SPEED_CHIPS as percent (percent)}
      <button type="button" class:on={current?.percent === percent} aria-pressed={current?.percent === percent} onclick={() => apply(percent)}>{speedLabel(percent)}</button>
    {/each}
  </div>
  <div class="row">
    <input
      type="text"
      inputmode="decimal"
      placeholder={t('video.speed.custom_placeholder')}
      aria-label={t('video.speed.custom')}
      aria-invalid={text !== '' && typed === null}
      bind:value={text}
    />
  </div>
  {#if text !== '' && typed === null}
    <p class="error" role="alert">{t('video.speed.invalid')}</p>
  {/if}
  <div class="row">
    <button type="button" class:on={keepFrames} aria-pressed={keepFrames} onclick={() => (keepFrames = true)}>{t('video.speed.keep_frames')}</button>
    <button type="button" class:on={!keepFrames} aria-pressed={!keepFrames} onclick={() => (keepFrames = false)}>{t('video.speed.drop_frames')}</button>
  </div>
  <div class="row end">
    <button type="button" onclick={() => apply(null)}>{t('video.speed.reset')}</button>
    <button type="button" class="apply" disabled={typed === null} onclick={() => typed !== null && apply(typed)}>{t('video.speed.apply')}</button>
  </div>
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
  .row.end {
    justify-content: space-between;
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
    width: 100px;
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
    padding: 3px 12px;
    font-weight: 600;
    background: #0a84ff;
  }
  .apply:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
