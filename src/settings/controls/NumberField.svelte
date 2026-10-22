<script lang="ts">
  let {
    value,
    min,
    max,
    unit,
    onchange,
    labelledby,
    disabled = false,
  }: {
    value: number;
    min: number;
    max: number;
    unit: string;
    onchange: (value: number) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();

  function commit(input: HTMLInputElement) {
    const typed = Number(input.value);
    if (!Number.isFinite(typed)) {
      input.value = String(value);
      return;
    }
    const clamped = Math.min(max, Math.max(min, Math.round(typed)));
    input.value = String(clamped);
    if (clamped !== value) onchange(clamped);
  }
</script>

<span class="field">
  <input
    type="number"
    {min}
    {max}
    {value}
    {disabled}
    aria-labelledby={labelledby}
    onchange={(event) => commit(event.currentTarget)}
  />
  <span class="unit">{unit}</span>
</span>

<style>
  .field {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  input {
    width: 72px;
    padding: 4px 6px;
    font: inherit;
    font-variant-numeric: tabular-nums;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  input:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  .unit {
    color: var(--fg-2);
  }
</style>
