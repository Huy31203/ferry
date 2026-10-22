<script lang="ts">
  let {
    value,
    min,
    max,
    onchange,
    labelledby,
    disabled = false,
  }: {
    value: number;
    min: number;
    max: number;
    onchange: (value: number) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();

  // The thumb moves freely; the value is saved when it is let go.
  let live = $state(0);
  $effect(() => {
    live = value;
  });
</script>

<div class="slider">
  <input
    type="range"
    {min}
    {max}
    {disabled}
    bind:value={live}
    aria-labelledby={labelledby}
    onchange={() => onchange(live)}
  />
  <output>{live}</output>
</div>

<style>
  .slider {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  input {
    width: 160px;
    accent-color: var(--accent);
  }
  output {
    min-width: 2.5em;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
