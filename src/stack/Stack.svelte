<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { draggable, hoverTarget, isBusy, openTool, remove, upsert, type OpenTool, type Tool } from '../lib/cards';
  import type { CropFrame } from '../lib/edits';
  import type { CardAction, EditsPatch, JobUpdate, StackConfig } from '../lib/ipc';
  import type { Settings } from '../lib/settings';
  import type { UpdateView } from '../lib/svc';
  import { DEFAULT_RAIL_TOOLS } from '../lib/rail';
  import { isVideo } from '../lib/video'; // stream A: video
  import { nextListMode } from '../lib/selection'; // Stream D
import { shareAnchor } from '../lib/share'; // Stream H
  import type { ListConfig } from '../lib/drop-ipc'; // Stream D
  import Card from './Card.svelte';
  import CompactList from './CompactList.svelte'; // Stream D
  import CropPopover from './CropPopover.svelte';
  import FitPopover from './FitPopover.svelte';
  import PdfCropPopover from './PdfCropPopover.svelte'; // sE pdf-formats
  import SpeedPopover from './SpeedPopover.svelte'; // stream A: video
  import DragAllBar from './DragAllBar.svelte';
  import UpdatePill from './UpdatePill.svelte';

  let cards = $state<JobUpdate[]>([]);
  let config = $state<StackConfig>({
    corner: 'bottom_right',
    card_size: 'default',
    auto_hide_secs: 6,
    compression: 80,
    crop_sizes: [],
    hover_hotkeys: true,
    rail: [...DEFAULT_RAIL_TOOLS],
    // stream A: video
    video_compression: 80,
    speed_frames: 'keep',
  });
  /** svc: the updater's state, for the "Update available" pill. */
  let update = $state<UpdateView | null>(null);
  let hovered = $state<string | null>(null);
  /** The button the pointer is on: its card (null for the Drag all bar) and its `data-action`. */
  let hot = $state<{ card: string | null; action: string } | null>(null);
  let overBar = $state(false);
  let dragging = $state(0);
  // Pointer on the Drag all bar, or a drag under way: no card may fade out from under the user.
  const held = $derived(overBar || dragging > 0);
  let root = $state<HTMLDivElement>();

  /** The one card tool that has the keyboard (rename, crop, fit, a slider drag). */
  let tool = $state<OpenTool | null>(null);

  // A tool whose card left (closed, faded, cleared, now busy) closes with it,
  // so the stack never keeps the keyboard for a card that is gone.
  $effect(() => {
    if (tool !== null && openTool(tool, cards) === null) tool = null;
  });

  // The stack takes the keyboard only while a tool is open (spec §2).
  let keyed = false;
  $effect(() => {
    const on = tool !== null || listFocused;
    if (on === keyed) return;
    keyed = on;
    invoke('stack_focus', { on }).catch((e) => console.warn('stack_focus', e));
  });

  // Hover hotkeys follow the idle card under the pointer (Rust registers them).
  let hoverSent: string | null = null;
  $effect(() => {
    const id = hoverTarget(hovered, cards, tool, config.hover_hotkeys);
    if (id === hoverSent) return;
    hoverSent = id;
    invoke('card_hover', { id }).catch((e) => console.warn('card_hover', e));
  });

  function setTool(id: string, kind: Tool | null) {
    if (kind !== null) tool = { id, kind };
    else if (tool?.id === id) tool = null;
  }

  /** The crop popover's live frame, drawn on its card's thumbnail. */
  let frame = $state<CropFrame | null>(null);

  // Stream D: above `listAfter` cards the stack becomes a compact list, but the
  // layout never flips under the pointer, a selection, an open tool or a drag.
  let listAfter = $state(5);
  let listMode = $state(false);
  let overList = $state(false);
  /** The user clicked the list, so it has the keyboard (never from hovering). */
  let listFocused = $state(false);
  /** The cards a crop from the selection bar applies to, instead of one card. */
  let cropFor = $state<string[] | null>(null);
  $effect(() => {
    const hold = held || overList || tool !== null || listFocused;
    listMode = nextListMode(cards.length, listAfter, untrack(() => listMode), hold);
  });

  // Crop or Fit: close the popover (handing the keyboard back), then run the edit.
  function apply(id: string, edits: EditsPatch) {
    setTool(id, null);
    // Stream D: a crop from the selection bar applies to every selected card.
    if (cropFor) {
      const ids = cropFor;
      cropFor = null;
      invoke('list_edit', { ids, edits }).catch((e) => console.warn('list_edit', e));
      return;
    }
    invoke('card_edit', { id, edits }).catch((e) => console.warn('card_edit', e));
  }

  // Stream D: the list gives the keyboard back when it goes away; a crop for the selection ends with its popover.
  $effect(() => {
    if (!listMode || cards.length === 0) listFocused = false;
  });
  $effect(() => {
    if (tool === null) cropFor = null;
  });

  // A click elsewhere in the stack closes a popover (spec §5.3 "click-away").
  function clickAway(event: PointerEvent) {
    if (tool === null || (tool.kind !== 'crop' && tool.kind !== 'fit' && tool.kind !== 'speed')) return;
    const target = event.target as Element;
    if (target.closest('[data-popover]') || target.closest(`[data-action="${tool.kind}"]`)) return;
    tool = null;
  }

  // Another app took the keyboard: a popover or a rename field closes.
  function windowBlur() {
    listFocused = false; // Stream D
    if (tool !== null && tool.kind !== 'slider') tool = null;
  }

  // A menu item or hover key that ends here. open_compare and quick_preview
  // reject when the card can't be shown (file gone, still running); that is only logged.
  function cardAction({ id, action }: CardAction) {
    const card = cards.find((c) => c.id === id);
    if (!card) return;
    if (action === 'compare' || action === 'preview') {
      invoke(action === 'compare' ? 'open_compare' : 'quick_preview', { id }).catch((e) =>
        console.warn(action, e),
      );
    } else if (action === 'close') {
      if (isBusy(card.state)) invoke('card_cancel', { id });
      close(id);
    } else if (action === 'share') {
      const el = root?.querySelector<HTMLElement>(`article[data-card-id="${id}"]`);
      if (el) invoke('card_share', { id, anchor: shareAnchor(el.getBoundingClientRect(), railRight) }).catch((e) => console.warn('share', e));
    } else {
      setTool(id, action);
    }
  }

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
    overList = el?.closest('[data-list]') != null; // Stream D
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
    overList = false; // Stream D
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
      listen<CardAction>('card:action', (event) => cardAction(event.payload)),
      listen<Settings>('settings:changed', (event) => {
        const { floating, images, video } = event.payload;
        listAfter = event.payload.results_list.after_cards; // Stream D
        config = {
          corner: floating.corner,
          card_size: floating.card_size,
          auto_hide_secs: floating.auto_hide_secs,
          compression: images.compression,
          crop_sizes: images.crop_sizes,
          hover_hotkeys: floating.hover_hotkeys,
          rail: event.payload.svc.rail.tools,
          // stream A: video
          video_compression: video.compression,
          speed_frames: video.speed_frames,
        };
      }),
      // A restored keep-both copy is gone, and its card goes with it.
      listen<string>('job:close', (event) => {
        cards = remove(cards, event.payload);
      }),
      listen('stack:clear', () => {
        cards = [];
      }),
      listen<UpdateView>('svc:update', (event) => {
        update = event.payload;
      }),
    ]).then(async (stops) => {
      const [stackConfig, pending] = await invoke<[StackConfig, JobUpdate[]]>('stack_ready');
      config = stackConfig;
      for (const card of pending) cards = upsert(cards, card);
      return stops;
    });
    invoke<ListConfig>('list_config') // Stream D
      .then((list) => (listAfter = list.after_cards))
      .catch((e) => console.warn('list_config', e));
    invoke<UpdateView>('svc_update_state')
      .then((view) => (update = view))
      .catch((e) => console.warn('svc_update_state', e));
    const observer = new ResizeObserver(report);
    if (root) observer.observe(root);
    return () => {
      observer.disconnect();
      unlisten.then((stops) => stops.forEach((stop) => stop()));
    };
  });

  function close(id: string) {
    cards = remove(cards, id);
  }

  function clearAll() {
    cards = [];
    invoke('clear_all');
  }
</script>

<svelte:window onblur={windowBlur} />

<!-- Native pointer events cover Windows; on macOS the panel never gets them and Rust forwards the pointer instead. -->
<div
  class="stack"
  class:top
  class:left={railRight}
  bind:this={root}
  role="presentation"
  onpointermove={(event) => track(event.target as Element)}
  onpointerleave={leave}
  onpointerdown={clickAway}
>
  <!-- The bar sits at the screen-edge end of the stack, because an overflowing stack is clipped at the far end. -->
  {#if cards.length > 0 && top && update?.pill}
    <UpdatePill view={update} hotAction={hot && hot.card === null ? hot.action : null} />
  {/if}
  {#if cards.length > 1 && top && !listMode}
    <DragAllBar
      cards={draggable(cards)}
      hotAction={hot && hot.card === null ? hot.action : null}
      above={!top}
      onclear={clearAll}
      ondrag={dragState}
    />
  {/if}
  {#if listMode}
    <!-- Stream D: the same cards as a list; its popovers open beside it like a card's. -->
    {@const owner = tool ? cards.find((c) => c.id === tool?.id) : undefined}
    <div class="row">
      <CompactList
        {cards}
        {hovered}
        {held}
        {overList}
        bind:focused={listFocused}
        hotAction={hot}
        barAction={hot && hot.card === null ? hot.action : null}
        {tool}
        autoHideSecs={config.auto_hide_secs}
        onclose={close}
        onclear={clearAll}
        oncrop={(ids, first) => {
          cropFor = ids;
          setTool(first, 'crop');
        }}
        ondrag={dragState}
        ontool={setTool}
      />
      {#if owner && tool?.kind === 'crop' && owner.source_format === 'pdf'}
        <!-- sE pdf-formats -->
        <PdfCropPopover
          card={owner}
          onapply={(edits) => apply(owner.id, edits)}
          onclose={() => setTool(owner.id, null)}
          onframe={(next) => (frame = next)}
        />
      {:else if owner && tool?.kind === 'crop'}
        <CropPopover
          card={owner}
          sizes={config.crop_sizes}
          onapply={(edits) => apply(owner.id, edits)}
          onclose={() => setTool(owner.id, null)}
          onframe={(next) => (frame = next)}
        />
      {:else if owner && tool?.kind === 'fit'}
        <FitPopover card={owner} onapply={(edits) => apply(owner.id, edits)} onclose={() => setTool(owner.id, null)} />
      {:else if owner && tool?.kind === 'speed'}
        <SpeedPopover
          card={owner}
          framesDefault={config.speed_frames}
          onapply={(edits) => apply(owner.id, edits)}
          onclose={() => setTool(owner.id, null)}
        />
      {/if}
    </div>
  {:else}
  {#each cards as card (card.id)}
    {@const open = tool?.id === card.id ? tool.kind : null}
    <!-- A popover opens beside its card, toward the screen centre; the window grows to fit (spec §5.3). -->
    <div class="row">
      <Card
        {card}
        hovered={card.id === hovered}
        {held}
        hotAction={hot?.card === card.id ? hot.action : null}
        compact={config.card_size === 'compact'}
        {railRight}
        autoHideSecs={config.auto_hide_secs}
        compression={isVideo(card.format) ? config.video_compression : config.compression}
        railTools={config.rail}
        tool={open}
        onclose={() => close(card.id)}
        ondrag={dragState}
        ontool={(kind) => setTool(card.id, kind)}
        frame={open === 'crop' ? frame : null}
      />
      {#if open === 'crop' && card.source_format === 'pdf'}
        <!-- sE pdf-formats -->
        <PdfCropPopover
          {card}
          onapply={(edits) => apply(card.id, edits)}
          onclose={() => setTool(card.id, null)}
          onframe={(next) => (frame = next)}
        />
      {:else if open === 'crop'}
        <CropPopover
          {card}
          sizes={config.crop_sizes}
          onapply={(edits) => apply(card.id, edits)}
          onclose={() => setTool(card.id, null)}
          onframe={(next) => (frame = next)}
        />
      {:else if open === 'fit'}
        <FitPopover {card} onapply={(edits) => apply(card.id, edits)} onclose={() => setTool(card.id, null)} />
      {:else if open === 'speed'}
        <!-- stream A: video -->
        <SpeedPopover
          {card}
          framesDefault={config.speed_frames}
          onapply={(edits) => apply(card.id, edits)}
          onclose={() => setTool(card.id, null)}
        />
      {/if}
    </div>
  {/each}
  {/if}
  {#if cards.length > 1 && !top && !listMode}
    <DragAllBar
      cards={draggable(cards)}
      hotAction={hot && hot.card === null ? hot.action : null}
      above={!top}
      onclear={clearAll}
      ondrag={dragState}
    />
  {/if}
  {#if cards.length > 0 && !top && update?.pill}
    <UpdatePill view={update} hotAction={hot && hot.card === null ? hot.action : null} />
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
  /* Cards keep to the screen edge; a popover sits on their screen-centre side. */
  .row {
    display: flex;
    flex-direction: row-reverse;
    align-items: flex-start;
    gap: 8px;
  }
  .stack.left .row {
    flex-direction: row;
  }
</style>
