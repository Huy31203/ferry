<script lang="ts">
  /** The vertical slider a press-and-drag on Downscale or Compression drops over the thumbnail (spec §5.2). */
  let {
    index,
    count,
    label,
    left,
    track,
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
  } = $props();

  const knob = $derived((index / Math.max(1, count - 1)) * track);
</script>

<div class="slider" style="left: {left}px; height: {track}px" aria-hidden="true">
  <div class="fill" style="height: {knob}px"></div>
  <div class="knob" style="top: {knob}px"></div>
  <div class="badge" style="top: {knob}px">{label}</div>
</div>

<style>
  .slider {
    position: absolute;
    top: 44px;
    width: 6px;
    margin-left: -3px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.25);
    z-index: 4;
    pointer-events: none;
  }
  .fill {
    width: 100%;
    border-radius: 3px;
    background: var(--accent);
  }
  .knob {
    position: absolute;
    left: -5px;
    width: 16px;
    height: 16px;
    margin-top: -8px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.4);
  }
  .badge {
    position: absolute;
    left: 18px;
    margin-top: -11px;
    padding: 3px 8px;
    border-radius: 7px;
    background: rgba(30, 30, 32, 0.9);
    color: #fff;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }
</style>
