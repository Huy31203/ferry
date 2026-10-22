<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import type { CardData } from '../lib/ipc';
  import Card from './Card.svelte';

  let cards = $state<CardData[]>([]);
  let hovered = $state<string | null>(null);
  let root = $state<HTMLDivElement>();

  // Ask Rust to fit the window to the stack; 0×0 hides it. getBoundingClientRect
  // forces layout, so this works even while the window is still hidden.
  function report() {
    if (!root) return;
    const rect = root.getBoundingClientRect();
    const empty = cards.length === 0;
    invoke('stack_resize', {
      width: empty ? 0 : Math.ceil(rect.width),
      height: empty ? 0 : Math.ceil(rect.height),
    });
  }

  $effect(() => {
    void cards.length;
    report();
  });

  // Rust forwards the pointer because a never-key window gets no :hover. AppKit's
  // y runs up from the window's bottom edge.
  function pointAt(x: number, fromBottom: number) {
    const el = document.elementFromPoint(x, window.innerHeight - fromBottom);
    hovered = el?.closest<HTMLElement>('[data-card-id]')?.dataset.cardId ?? null;
  }

  onMount(() => {
    const unlisten = Promise.all([
      listen<CardData>('stack:test-card', (event) => {
        cards = [...cards, event.payload];
      }),
      listen<{ x: number; fromBottom: number }>('stack:pointer', (event) =>
        pointAt(event.payload.x, event.payload.fromBottom),
      ),
      listen('stack:pointer-leave', () => {
        hovered = null;
      }),
    ]);
    const observer = new ResizeObserver(report);
    if (root) observer.observe(root);
    return () => {
      observer.disconnect();
      unlisten.then((stops) => stops.forEach((stop) => stop()));
    };
  });

  function close(id: string) {
    cards = cards.filter((card) => card.id !== id);
  }
</script>

<div class="stack" bind:this={root}>
  {#each cards as card (card.id)}
    <Card {card} hovered={card.id === hovered} onclose={() => close(card.id)} />
  {/each}
</div>

<style>
  :global(html),
  :global(body) {
    background: transparent;
    height: 100%;
    overflow: hidden;
  }
  /* Bottom-anchored: when Rust clamps the window below the stack's height, the
     oldest cards are clipped at the top and the newest stay visible. */
  :global(#app) {
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    height: 100%;
  }
  .stack {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    width: max-content;
  }
</style>
