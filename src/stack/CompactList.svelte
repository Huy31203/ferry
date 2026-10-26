<script lang="ts">
  import { flushSync, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { canDrag, draggable, isBusy, type OpenTool, type Tool } from '../lib/cards';
  import { t } from '../lib/i18n';
  import { isMac, type JobUpdate } from '../lib/ipc';
  import {
    LIST_ROW,
    LIST_ROWS_MAX,
    NO_SELECTION,
    clearSel,
    escapeSel,
    extendTo,
    headerText,
    keyPatch,
    listKey,
    listTotals,
    moveCursor,
    prune,
    rowsHeld,
    selectAll,
    targets as targetsOf,
    toggle,
    type Sel,
  } from '../lib/selection';
  import { loadPdfConfig, pdfConfig } from '../lib/pdfConfig.svelte';
  import { visibleRange } from '../lib/virtual';
  import DragAllBar from './DragAllBar.svelte';
  import ListRow from './ListRow.svelte';
  import SelectionBar from './SelectionBar.svelte';

  onMount(() => {
    loadPdfConfig().catch((e) => console.warn('pdf_config', e));
  });

  /** Past this many rows only the ones in view are drawn. */
  const WINDOWED = 50;

  let {
    cards,
    hovered = null,
    held = false,
    overList = false,
    focused = $bindable(false),
    hotAction = null,
    barAction = null,
    tool = null,
    autoHideSecs,
    onclose,
    onclear,
    oncrop,
    ondrag,
    ontool,
  }: {
    cards: JobUpdate[];
    hovered?: string | null;
    held?: boolean;
    overList?: boolean;
    /** The list has the keyboard: the user clicked it (never from hovering). */
    focused?: boolean;
    /** The `data-action` of the button the pointer is on, for the row that owns it. */
    hotAction?: { card: string | null; action: string } | null;
    barAction?: string | null;
    tool?: OpenTool | null;
    autoHideSecs: number;
    onclose: (id: string) => void;
    onclear: () => void;
    /** Open the crop popover for the selection; `first` is the card it opens beside. */
    oncrop: (ids: string[], first: string) => void;
    ondrag: (active: boolean) => void;
    ontool: (id: string, kind: Tool | null) => void;
  } = $props();

  let sel = $state<Sel>(NO_SELECTION);
  let dragging = $state(0);
  let scrollTop = $state(0);
  let viewport = $state(LIST_ROW * LIST_ROWS_MAX);
  let body = $state<HTMLDivElement>();

  const totals = $derived(listTotals(cards));
  const picked = $derived(targetsOf(cards, sel.ids));
  const holding = $derived(rowsHeld(overList, sel.ids.length, tool !== null || focused, dragging > 0));
  const windowed = $derived(cards.length > WINDOWED);
  const range = $derived(
    windowed
      ? visibleRange(scrollTop, viewport, LIST_ROW, cards.length)
      : { start: 0, end: cards.length, padTop: 0, padBottom: 0 },
  );
  const shown = $derived(cards.slice(range.start, range.end));

  // Rows that left the list leave the selection.
  $effect(() => {
    sel = prune(sel, cards);
  });

  // New rows arrive at the bottom; follow them unless the pointer is on the list.
  $effect(() => {
    void cards.length;
    if (body && !overList) body.scrollTop = body.scrollHeight;
  });

  // The keyboard cursor stays in view while the list has the keyboard.
  $effect(() => {
    if (focused && sel.cursor) {
      document.getElementById(`row-${sel.cursor}`)?.scrollIntoView({ block: 'nearest' });
    }
  });

  /** A click on a checkbox, or on a row while something is selected: select and take the keyboard. */
  function pick(id: string, shift: boolean) {
    sel = shift ? extendTo(sel, cards, id) : toggle(sel, id);
    focused = true;
  }

  function bodyClick(event: MouseEvent) {
    if (!(event.target as Element).closest('[data-card-id]')) focused = true;
  }

  function dragState(active: boolean) {
    dragging = Math.max(0, dragging + (active ? 1 : -1));
    ondrag(active);
  }

  function crop() {
    if (picked.edit.length > 0) oncrop(picked.edit, picked.edit[0]);
  }

  function remove(id: string) {
    onclose(id);
  }

  /** Quick Look and compare take the keyboard themselves, so the list lets go first. */
  function release() {
    focused = false;
    flushSync();
  }

  function onkeydown(event: KeyboardEvent) {
    if (!focused || tool !== null) return;
    if ((event.target as Element).closest('input, textarea, select')) return;
    const key = listKey(event, isMac);
    if (!key) return;
    event.preventDefault();
    const row = cards.find((c) => c.id === sel.cursor);
    switch (key.kind) {
      case 'select_all':
        sel = selectAll(sel, cards);
        break;
      case 'escape': {
        const next = escapeSel(sel);
        sel = next.sel;
        if (next.release) focused = false;
        break;
      }
      case 'move':
        sel = moveCursor(sel, cards, key.delta, key.extend);
        break;
      case 'preview':
      case 'compare':
        if (row && canDrag(row)) {
          release();
          const command = key.kind === 'preview' ? 'quick_preview' : 'open_compare';
          invoke(command, { id: row.id }).catch((e) => console.warn(command, e));
        }
        break;
      case 'scale':
      case 'step_down': {
        if (!row || !row.can_edit || isBusy(row.state)) break;
        const edits = keyPatch(key, row, pdfConfig);
        if (edits) invoke('card_edit', { id: row.id, edits }).catch((e) => console.warn('card_edit', e));
        break;
      }
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="list" data-list>
  {#if sel.ids.length > 0}
    <SelectionBar
      {cards}
      ids={sel.ids}
      targets={picked}
      hotAction={barAction}
      onall={() => (sel = selectAll(sel, cards))}
      onclear={() => (sel = clearSel(sel))}
      oncrop={crop}
      onremove={remove}
      oncopied={() => (focused = false)}
      ondrag={dragState}
    />
  {:else}
    <div class="header">{headerText(totals)}</div>
    {#if cards.length > 1}
      <DragAllBar cards={draggable(cards)} hotAction={barAction} {onclear} ondrag={dragState} />
    {/if}
  {/if}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="body"
    role="listbox"
    tabindex="-1"
    aria-multiselectable="true"
    aria-label={t('drop.list.aria')}
    aria-activedescendant={focused && sel.cursor ? `row-${sel.cursor}` : undefined}
    style:max-height="{LIST_ROW * LIST_ROWS_MAX}px"
    bind:this={body}
    bind:clientHeight={viewport}
    onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
    onclick={bodyClick}
  >
    <div style:padding-top="{range.padTop}px" style:padding-bottom="{range.padBottom}px">
      {#each shown as card (card.id)}
        <ListRow
          {card}
          hovered={card.id === hovered}
          held={held || holding}
          selecting={sel.ids.length > 0}
          checked={sel.ids.includes(card.id)}
          ring={focused && sel.cursor === card.id}
          hotAction={hotAction?.card === card.id ? hotAction.action : null}
          {autoHideSecs}
          tool={tool?.id === card.id ? tool.kind : null}
          onclose={() => onclose(card.id)}
          ondrag={dragState}
          ontool={(kind) => ontool(card.id, kind)}
          ontoggle={(shift) => pick(card.id, shift)}
        />
      {/each}
    </div>
  </div>
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 320px;
    box-sizing: border-box;
  }
  .header {
    padding: 6px 10px;
    border-radius: 10px;
    background: var(--rail-bg);
    color: var(--rail-fg);
    font-size: 12px;
    font-weight: 600;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2), 0 0 0 0.5px var(--rail-line);
  }
  .body {
    overflow-y: auto;
    border-radius: 10px;
    background: var(--rail-bg);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2), 0 0 0 0.5px var(--rail-line);
    outline: none;
  }
</style>
