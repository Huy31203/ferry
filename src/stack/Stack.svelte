<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { draggable, upsert } from '../lib/cards';
  import type { JobUpdate, StackConfig } from '../lib/ipc';
  import type { Settings } from '../lib/settings';
  import Card from './Card.svelte';
  import DragAllBar from './DragAllBar.svelte';

  let cards = $state<JobUpdate[]>([]);
  let config = $state<StackConfig>({
    corner: 'bottom_right',
    card_size: 'default',
    auto_hide_secs: 6,
    compression: 80,
    crop_sizes: [],
    hover_hotkeys: true,
  });
  let hovered = $state<string | null>(null);
  /** The button the pointer is on: its card (null for the Drag all bar) and its `data-action`. */
  let hot = $state<{ card: string | null; action: string } | null>(null);
  let overBar = $state(false);
  let dragging = $state(0);
  // Pointer on the Drag all bar, or a drag under way: no card may fade out from under the user.
  const held = $derived(overBar || dragging > 0);
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

  // A new corner or card size changes where the window sits and how big it is.
  $effect(() => {
    void config.corner;
    void config.card_size;
    report();
  });

  // Rust forwards the pointer because a never-key window gets no :hover. AppKit's
  // y runs up from the window's bottom edge.
  function track(el: Element | null) {
    hovered = el?.closest<HTMLElement>('[data-card-id]')?.dataset.cardId ?? null;
    overBar = el?.closest('[data-bar]') != null;
    const button = el?.closest<HTMLElement>('[data-action]');
    hot = button
      ? { card: button.closest<HTMLElement>('[data-card-id]')?.dataset.cardId ?? null, action: button.dataset.action as string }
      : null;
  }

  function pointAt(x: number, fromBottom: number) {
    track(document.elementFromPoint(x, window.innerHeight - fromBottom));
  }

  function leave() {
    hovered = null;
    overBar = false;
    hot = null;
  }

  function dragState(active: boolean) {
    dragging = Math.max(0, dragging + (active ? 1 : -1));
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
      listen('stack:pointer-leave', leave),
      listen<Settings>('settings:changed', (event) => {
        const { floating, images } = event.payload;
        config = {
          corner: floating.corner,
          card_size: floating.card_size,
          auto_hide_secs: floating.auto_hide_secs,
          compression: images.compression,
          crop_sizes: images.crop_sizes,
          hover_hotkeys: floating.hover_hotkeys,
        };
      }),
      listen('stack:clear', () => {
        cards = [];
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

<!-- Native pointer events cover Windows; on macOS the panel never gets them and Rust forwards the pointer instead. -->
<div
  class="stack"
  class:top
  bind:this={root}
  role="presentation"
  onpointermove={(event) => track(event.target as Element)}
  onpointerleave={leave}
>
  <!-- The bar sits at the screen-edge end of the stack, because an overflowing stack is clipped at the far end. -->
  {#if cards.length > 1 && top}
    <DragAllBar
      cards={draggable(cards)}
      hotAction={hot && hot.card === null ? hot.action : null}
      above={!top}
      onclear={clearAll}
      ondrag={dragState}
    />
  {/if}
  {#each cards as card (card.id)}
    <Card
      {card}
      hovered={card.id === hovered}
      {held}
      hotAction={hot?.card === card.id ? hot.action : null}
      compact={config.card_size === 'compact'}
      {railRight}
      autoHideSecs={config.auto_hide_secs}
      onclose={() => close(card.id)}
      ondrag={dragState}
    />
  {/each}
  {#if cards.length > 1 && !top}
    <DragAllBar
      cards={draggable(cards)}
      hotAction={hot && hot.card === null ? hot.action : null}
      above={!top}
      onclear={clearAll}
      ondrag={dragState}
    />
  {/if}
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
