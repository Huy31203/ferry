<script lang="ts">
  /** The vertical slider a press-and-drag on Downscale or Compression drops over the thumbnail (spec §5.2). */
  let {
    index,
    count,
    label,
    left,
    track,
    width = 300,
  }: {
    /** The stop under the pointer, 0 at the top. */
    index: number;
    count: number;
    /** The live target, such as "50 % · 1440×900" or "q 60". */
    label: string;
    /** The centre of the button it drops from, in card pixels. */
    left: number;
    /** The track's length in px. */
    track: number;
    /** The card's width in px, so the panel stays inside it. */
    width?: number;
  } = $props();

  /** Panel padding; the vertical one leaves room for the badge's half-height at the first and last stop. */
  const PAD_X = 10;
  const PAD_Y = 12;
  const KNOB = 16;
  const EDGE = 6;

  const knob = $derived((index / Math.max(1, count - 1)) * track);
  let panelWidth = $state(0);
  // The track column's centre sits PAD_X + KNOB / 2 from the panel's near edge: badge to the right, or to the left with no room.
  const toRight = $derived(left - PAD_X - KNOB / 2 + panelWidth <= width - EDGE);
  const x = $derived(
    Math.max(EDGE, Math.min(width - EDGE - panelWidth, toRight ? left - PAD_X - KNOB / 2 : left + PAD_X + KNOB / 2 - panelWidth)),
  );
</script>

<div
  class="slider"
  class:flip={!toRight}
  style="left: {x}px; padding: {PAD_Y}px {PAD_X}px"
  bind:clientWidth={panelWidth}
  aria-hidden="true"
>
  <div class="track" style="height: {track}px; width: {KNOB}px">
    <div class="rail-line"></div>
    <div class="fill" style="height: {knob}px"></div>
    <div class="knob" style="top: {knob}px"></div>
  </div>
  <div class="badge" style="transform: translateY({knob - 11}px)">{label}</div>
</div>

<style>
  .slider {
    position: absolute;
    top: 46px;
    display: flex;
    align-items: flex-start;
    gap: 8px;
    border-radius: 12px;
    background: var(--rail-bg);
    color: var(--rail-fg);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25), 0 0 0 0.5px var(--rail-line);
    -webkit-backdrop-filter: blur(12px);
    backdrop-filter: blur(12px);
    z-index: 4;
    pointer-events: none;
  }
  .slider.flip {
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
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.4), 0 0 0 0.5px rgba(0, 0, 0, 0.3);
  }
  .badge {
    min-width: 112px;
    height: 22px;
    padding: 0 8px;
    box-sizing: border-box;
    border-radius: 7px;
    background: var(--rail-line);
    font-size: 12px;
    font-weight: 600;
    line-height: 22px;
    white-space: nowrap;
  }
</style>
