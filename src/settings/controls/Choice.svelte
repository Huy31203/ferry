<script lang="ts">
  let {
    value,
    options,
    onchange,
    labelledby,
    disabled = false,
  }: {
    value: string;
    options: { value: string; label: string }[];
    onchange: (value: string) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();
</script>

<select
  {disabled}
  aria-labelledby={labelledby}
  onchange={(event) => onchange(event.currentTarget.value)}
>
  {#each options as option (option.value)}
    <option value={option.value} selected={option.value === value}>{option.label}</option>
  {/each}
</select>

<style>
  select {
    padding: 4px 8px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  select:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
</style>
