<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { dragAllFiles, upsert } from '../lib/cards';
  import type { JobUpdate, StackConfig } from '../lib/ipc';
  import Card from './Card.svelte';
  import DragAllBar from './DragAllBar.svelte';

  let cards = $state<JobUpdate[]>([]);
  let config = $state<StackConfig>({ corner: 'bottom_right', card_size: 'default', auto_hide_secs: 6 });
  let hovered = $state<string | null>(null);
  let root = $state<HTMLDivElement>();

  const top = $derived(config.corner.startsWith('top'));
  const railRight = $derived(config.corner.endsWith('left'));

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
    // Listen first, then tell Rust we are ready, so no card falls in between.
    const unlisten = Promise.all([
      listen<JobUpdate>('job:update', (event) => {
        cards = upsert(cards, event.payload);
      }),
      listen<{ x: number; fromBottom: number }>('stack:pointer', (event) =>
        pointAt(event.payload.x, event.payload.fromBottom),
      ),
      listen('stack:pointer-leave', () => {
        hovered = null;
      }),
    ]).then(async (stops) => {
      const [stackConfig, pending] = await invoke<[StackConfig, JobUpdate[]]>('stack_ready');
      config = stackConfig;
      for (const card of pending) cards = upsert(cards, card);
      return stops;
    });
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

  function clearAll() {
    cards = [];
    invoke('clear_all');
  }
</script>

<div class="stack" class:top bind:this={root}>
  {#if cards.length > 1}
    <DragAllBar files={dragAllFiles(cards)} onclear={clearAll} />
  {/if}
  {#each cards as card (card.id)}
    <Card
      {card}
      hovered={card.id === hovered}
      compact={config.card_size === 'compact'}
      {railRight}
      autoHideSecs={config.auto_hide_secs}
      onclose={() => close(card.id)}
    />
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
  :global(#app:has(.stack.top)) {
    justify-content: flex-start;
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
