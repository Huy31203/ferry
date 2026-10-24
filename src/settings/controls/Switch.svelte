<script lang="ts">
  let {
    checked,
    onchange,
    labelledby,
    disabled = false,
    label,
  }: {
    checked: boolean;
    onchange: (value: boolean) => void;
    labelledby: string;
    disabled?: boolean;
    /** svc: an accessible name of its own, instead of the row's label. */
    label?: string;
  } = $props();
</script>

<button
  type="button"
  role="switch"
  class="switch"
  class:on={checked}
  aria-checked={checked}
  aria-label={label}
  aria-labelledby={label ? undefined : labelledby}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span class="knob"></span>
</button>

<style>
  .switch {
    flex-shrink: 0;
    width: 38px;
    height: 22px;
    padding: 2px;
    border: 0;
    border-radius: 11px;
    background: var(--track-off);
    cursor: pointer;
    transition: background 120ms;
  }
  .switch.on {
    background: var(--accent);
  }
  .switch:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .switch:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 2px;
  }
  .knob {
    display: block;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #fff;
    transition: transform 120ms;
  }
  .on .knob {
    transform: translateX(16px);
  }
  @media (prefers-reduced-motion: reduce) {
    .switch,
    .knob {
      transition: none;
    }
  }
</style>
