<script lang="ts">
  import { t, type StringKey } from '../../lib/i18n';

  // stream A: video. Three cards for how videos are re-encoded.
  let {
    value,
    onchange,
    labelledby,
    disabled = false,
  }: {
    value: string;
    onchange: (value: string) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();

  const CHOICES = ['fast', 'small', 'visually_lossless'];

  // Arrow keys move the choice, and the focus with it.
  function keydown(event: KeyboardEvent) {
    const step = ['ArrowRight', 'ArrowDown'].includes(event.key) ? 1 : ['ArrowLeft', 'ArrowUp'].includes(event.key) ? -1 : 0;
    if (step === 0 || disabled) return;
    event.preventDefault();
    const next = (CHOICES.indexOf(value) + step + CHOICES.length) % CHOICES.length;
    onchange(CHOICES[next]);
    const buttons = (event.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('button');
    buttons[next]?.focus();
  }
</script>

<!-- svelte-ignore a11y_interactive_supports_focus -->
<div class="cards" role="radiogroup" aria-labelledby={labelledby} onkeydown={keydown}>
  {#each CHOICES as choice (choice)}
    <button
      type="button"
      role="radio"
      aria-checked={value === choice}
      tabindex={value === choice ? 0 : -1}
      class:selected={value === choice}
      {disabled}
      onclick={() => onchange(choice)}
    >
      <span class="title">{t(`video.encoder.${choice}` as StringKey)}</span>
      <span class="help">{t(`video.encoder.${choice}.help` as StringKey)}</span>
    </button>
  {/each}
</div>

<style>
  .cards {
    display: grid;
    grid-template-columns: repeat(3, 112px);
    gap: 8px;
  }
  button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 8px;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: var(--bg-content);
    border: 2px solid transparent;
    border-radius: 8px;
    cursor: pointer;
  }
  button.selected {
    border-color: var(--accent);
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  button:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 2px;
  }
  .title {
    font-size: 13px;
    font-weight: 600;
  }
  .help {
    font-size: 11px;
    color: var(--fg-2);
  }
</style>
