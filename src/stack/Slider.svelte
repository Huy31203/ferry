<script lang="ts">
  import { SLIDER_INSET, SLIDER_TOP } from '../lib/edits';

  /** The vertical slider a press-and-drag on Downscale or Compression drops over the thumbnail (spec §5.2). */
  let {
    index,
    ys,
    label,
    stops,
    labelled,
    left,
    track,
    width = 300,
  }: {
    /** The stop under the pointer, 0 at the top. */
    index: number;
    /** Every stop's distance from the track's top in px: the track is the value axis, so round values sit evenly. */
    ys: number[];
    /** The live target, such as "50 % · 1440×900" or "q 60". */
    label: string;
    /** Every stop's short value, top to bottom, such as "50 %" or "q 60". */
    stops: string[];
    /** The stops that get a value label, from `labelledStops`. */
    labelled: number[];
    /** The centre of the button it drops from, in card pixels. */
    left: number;
    /** The track's length in px. */
    track: number;
    /** The card's width in px, so the panel stays inside it. */
    width?: number;
  } = $props();

  /** Panel padding; the readout and the insets above and below the track add the rest (SLIDER_CHROME). */
  const PAD = 8;
  const KNOB = 14;
  const EDGE = 6;
  /** A value chip's height; it is centred on its stop. */
  const CHIP = 16;

  const knob = $derived(ys[index] ?? 0);
  let panelWidth = $state(0);
  // The live line grows with its text; keep the widest it has been so the panel does not jitter while dragging.
  let lineWidth = $state(0);
  let lineMax = $state(0);
  $effect(() => {
    if (lineWidth > lineMax) lineMax = lineWidth;
  });
  // The track column's centre sits PAD + KNOB / 2 from the panel's near edge: labels to the right, or to the left with no room.
  const toRight = $derived(left - PAD - KNOB / 2 + panelWidth <= width - EDGE);
  const x = $derived(
    Math.max(EDGE, Math.min(width - EDGE - panelWidth, toRight ? left - PAD - KNOB / 2 : left + PAD + KNOB / 2 - panelWidth)),
  );
</script>

<div
  class="slider"
  style="left: {x}px; top: {SLIDER_TOP}px; padding: {PAD}px"
  bind:clientWidth={panelWidth}
  aria-hidden="true"
>
  <div class="line" style="min-width: {lineMax}px" bind:clientWidth={lineWidth}>{label}</div>
  <div class="row" class:flip={!toRight} style="padding: {SLIDER_INSET}px 0">
    <div class="track" style="height: {track}px; width: {KNOB}px">
      <div class="rail-line"></div>
      <div class="fill" style="height: {knob}px"></div>
      <div class="knob" style="top: {knob}px"></div>
    </div>
    <div class="marks" style="height: {track}px">
      {#each ys as y, i (i)}
        <div class="tick" class:long={labelled.includes(i)} style="top: {y - 1}px"></div>
      {/each}
    </div>
    <div class="chips" style="height: {track}px">
      {#each labelled as i (i)}
        <div class="chip" style="transform: translateY({ys[i] - CHIP / 2}px)">{stops[i]}</div>
      {/each}
    </div>
  </div>
</div>

<style>
  .slider {
    position: absolute;
    display: flex;
    flex-direction: column;
    border-radius: 8px;
    background: var(--rail-bg);
    color: var(--rail-fg);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25), 0 0 0 0.5px var(--rail-line);
    -webkit-backdrop-filter: blur(12px);
    backdrop-filter: blur(12px);
    z-index: 4;
    pointer-events: none;
  }
  .line {
    box-sizing: border-box;
    width: max-content;
    height: 16px;
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    line-height: 16px;
    white-space: nowrap;
  }
  .row {
    display: flex;
    align-items: flex-start;
    align-self: flex-start;
  }
  .row.flip {
    flex-direction: row-reverse;
    align-self: flex-end;
  }
  .track {
    position: relative;
    flex: none;
  }
  .rail-line,
  .fill {
    position: absolute;
    left: 5px;
    top: 0;
    width: 4px;
    border-radius: 2px;
  }
  .rail-line {
    height: 100%;
    background: var(--rail-line);
  }
  .fill {
    background: var(--accent);
  }
  .knob {
    position: absolute;
    left: 0;
    width: 14px;
    height: 14px;
    margin-top: -7px;
    border-radius: 50%;
    background: #fff;
    box-shadow:
      0 1px 4px rgba(0, 0, 0, 0.4),
      0 0 0 0.5px rgba(0, 0, 0, 0.3),
      0 0 0 3px color-mix(in srgb, var(--accent) 40%, transparent);
  }
  /* The ticks start 3 px off the 4 px track (centred in the 14 px knob column): the column overlaps it by 2 px. */
  .marks {
    position: relative;
    flex: none;
    width: 8px;
    margin-left: -2px;
    margin-right: 4px;
  }
  .row.flip .marks {
    margin-left: 4px;
    margin-right: -2px;
  }
  .tick {
    position: absolute;
    left: 0;
    width: 5px;
    height: 2px;
    border-radius: 1px;
    background: var(--rail-line);
  }
  .tick.long {
    width: 8px;
    background: var(--rail-fg);
    opacity: 0.55;
  }
  .row.flip .tick {
    left: auto;
    right: 0;
  }
  /* One grid cell: every chip is as wide as the widest, so the labels read as one aligned block. */
  .chips {
    display: grid;
  }
  .chip {
    grid-area: 1 / 1;
    align-self: start;
    height: 16px;
    padding: 0 6px;
    box-sizing: border-box;
    border-radius: 8px;
    background: var(--rail-line);
    font-size: 11px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    line-height: 16px;
    text-align: left;
    white-space: nowrap;
  }
</style>
