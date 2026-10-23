<script lang="ts">
  import { convertRequest, formatChoices } from '../lib/edits';
  import type { Format, JobUpdate } from '../lib/ipc';

  /** PNG · JPEG · WebP chips in place of the detail line while the card is hovered (spec §5.4). */
  let {
    card,
    hotAction = null,
    onconvert,
  }: {
    card: JobUpdate;
    hotAction?: string | null;
    onconvert: (format: Format, keepBoth: boolean) => void;
  } = $props();

  const choices = $derived(formatChoices(card));

  // ⌥-click (Alt-click on Windows) keeps both: a new card with the other format.
  function click(format: Format, event: MouseEvent) {
    const request = convertRequest(card, format, event.altKey);
    if (request) onconvert(request.format, request.keepBoth);
  }
</script>

<div class="bar">
  {#each choices as choice (choice.format)}
    <button
      type="button"
      data-action={`format-${choice.format}`}
      class:current={choice.current}
      class:hot={hotAction === `format-${choice.format}`}
      aria-pressed={choice.current}
      disabled={!card.can_edit}
      onclick={(event) => click(choice.format, event)}>{choice.label}</button
    >
  {/each}
</div>

<style>
  .bar {
    display: flex;
    gap: 4px;
  }
  button {
    border: 0;
    border-radius: 9px;
    padding: 1px 8px;
    font: inherit;
    font-size: 11px;
    font-weight: 600;
    color: #fff;
    background: rgba(255, 255, 255, 0.14);
    cursor: pointer;
  }
  button:hover:not(:disabled),
  button.hot:not(:disabled) {
    background: rgba(255, 255, 255, 0.3);
  }
  button.current {
    background: rgba(255, 255, 255, 0.9);
    color: #111;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
