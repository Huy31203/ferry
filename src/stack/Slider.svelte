<script lang="ts">
  import { SLIDER_TOP } from '../lib/edits';

  /** The vertical slider a press-and-drag on Downscale or Compression drops over the thumbnail (spec §5.2). */
  let {
    index,
    count,
    label,
    stops,
    labelled,
    left,
    track,
    width = 300,
  }: {
    /** The stop under the pointer, 0 at the top. */
    index: number;
    count: number;
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

  /** Panel padding; the readout, divider and the track's knob room add the rest (SLIDER_CHROME). */
  const PAD = 10;
  const KNOB = 16;
  const EDGE = 6;
  /** A value chip's height; it is centred on its stop. */
  const CHIP = 13;

  const stopY = (i: number) => (i / Math.max(1, count - 1)) * track;
  const knob = $derived(stopY(index));
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
  <div class="row" class:flip={!toRight}>
    <div class="track" style="height: {track}px; width: {KNOB}px">
      <div class="rail-line"></div>
      <div class="fill" style="height: {knob}px"></div>
      <div class="knob" style="top: {knob}px"></div>
    </div>
    <div class="marks" style="height: {track}px">
      {#each stops as _, i (i)}
        <div class="tick" class:long={labelled.includes(i)} style="top: {stopY(i) - 1}px"></div>
      {/each}
    </div>
    <div class="chips" style="height: {track}px">
      {#each labelled as i (i)}
        <div class="chip" style="transform: translateY({stopY(i) - CHIP / 2}px)">{stops[i]}</div>
      {/each}
    </div>
  </div>
</div>

<style>
  .slider {
    position: absolute;
    display: flex;
    flex-direction: column;
    border-radius: 12px;
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
    height: 14px;
    font-size: 12px;
    font-weight: 600;
    line-height: 14px;
    white-space: nowrap;
  }
  .row {
    display: flex;
    align-items: flex-start;
    margin-top: 6px;
    padding: 9px 0;
    border-top: 1px solid var(--rail-line);
  }
  .row.flip {
    flex-direction: row-reverse;
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
    width: 6px;
    border-radius: 3px;
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
    width: 16px;
    height: 16px;
    margin-top: -8px;
    border-radius: 50%;
    background: #fff;
    box-shadow:
      0 1px 4px rgba(0, 0, 0, 0.4),
      0 0 0 0.5px rgba(0, 0, 0, 0.3),
      0 0 0 3px color-mix(in srgb, var(--accent) 40%, transparent);
  }
  /* The ticks start at the track's edge: the column overlaps the track's 5 px of knob room. */
  .marks {
    position: relative;
    flex: none;
    width: 12px;
    margin-left: -5px;
  }
  .row.flip .marks {
    margin-left: 0;
    margin-right: -5px;
  }
  .tick {
    position: absolute;
    left: 0;
    width: 6px;
    height: 2px;
    border-radius: 1px;
    background: var(--rail-line);
  }
  .tick.long {
    width: 10px;
    background: var(--rail-fg);
    opacity: 0.55;
  }
  .row.flip .tick {
    left: auto;
    right: 0;
  }
  .chips {
    display: grid;
  }
  .chip {
    grid-area: 1 / 1;
    align-self: start;
    height: 13px;
    padding: 0 6px;
    box-sizing: border-box;
    border-radius: 5px;
    background: var(--rail-line);
    font-size: 11px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    line-height: 13px;
    white-space: nowrap;
  }
</style>
