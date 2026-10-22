<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { t, type StringKey } from '../../lib/i18n';

  let {
    value,
    onchange,
    labelledby,
  }: { value: string; onchange: (value: string) => void; labelledby: string } = $props();

  interface Preview {
    style: 'default' | 'classic' | 'flat';
    png: number[];
  }

  let previews = $state<{ style: Preview['style']; url: string }[]>([]);

  onMount(() => {
    let urls: string[] = [];
    invoke<Preview[]>('tray_style_previews').then((list) => {
      previews = list.map((p) => {
        const url = URL.createObjectURL(new Blob([new Uint8Array(p.png)], { type: 'image/png' }));
        urls.push(url);
        return { style: p.style, url };
      });
    });
    return () => urls.forEach((url) => URL.revokeObjectURL(url));
  });
</script>

<div class="styles" role="radiogroup" aria-labelledby={labelledby}>
  {#each previews as preview (preview.style)}
    <button
      type="button"
      role="radio"
      aria-checked={value === preview.style}
      class:selected={value === preview.style}
      onclick={() => onchange(preview.style)}
    >
      <img src={preview.url} alt="" width="32" height="32" />
      <span>{t(`tray_style.${preview.style}` as StringKey)}</span>
    </button>
  {/each}
</div>

<style>
  .styles {
    display: flex;
    gap: 8px;
  }
  button {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px 10px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 2px solid transparent;
    border-radius: 8px;
    cursor: pointer;
  }
  button.selected {
    border-color: var(--accent);
  }
  button:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 2px;
  }
  /* The images are black template art; the menu bar tints them, and so do we. */
  img {
    filter: none;
  }
  @media (prefers-color-scheme: dark) {
    img {
      filter: invert(1);
    }
  }
</style>
