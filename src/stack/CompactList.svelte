<script lang="ts">
  import { draggable, type OpenTool, type Tool } from '../lib/cards';
  import { t } from '../lib/i18n';
  import type { JobUpdate } from '../lib/ipc';
  import { LIST_ROW, LIST_ROWS_MAX, headerText, listTotals, rowsHeld } from '../lib/selection';
  import { visibleRange } from '../lib/virtual';
  import DragAllBar from './DragAllBar.svelte';
  import ListRow from './ListRow.svelte';

  /** Past this many rows only the ones in view are drawn. */
  const WINDOWED = 50;

  let {
    cards,
    hovered = null,
    held = false,
    overList = false,
    hotAction = null,
    barAction = null,
    tool = null,
    autoHideSecs,
    onclose,
    onclear,
    ondrag,
    ontool,
  }: {
    cards: JobUpdate[];
    hovered?: string | null;
    held?: boolean;
    overList?: boolean;
    /** The `data-action` of the button the pointer is on, for the row that owns it. */
    hotAction?: { card: string | null; action: string } | null;
    barAction?: string | null;
    tool?: OpenTool | null;
    autoHideSecs: number;
    onclose: (id: string) => void;
    onclear: () => void;
    ondrag: (active: boolean) => void;
    ontool: (id: string, kind: Tool | null) => void;
  } = $props();

  let selected = $state(new Set<string>());
  let dragging = $state(0);
  let scrollTop = $state(0);
  let viewport = $state(LIST_ROW * LIST_ROWS_MAX);
  let body = $state<HTMLDivElement>();

  const totals = $derived(listTotals(cards));
  const holding = $derived(rowsHeld(overList, selected.size, tool !== null, dragging > 0));
  const windowed = $derived(cards.length > WINDOWED);
  const range = $derived(
    windowed
      ? visibleRange(scrollTop, viewport, LIST_ROW, cards.length)
      : { start: 0, end: cards.length, padTop: 0, padBottom: 0 },
  );
  const shown = $derived(cards.slice(range.start, range.end));

  // Rows that left the list leave the selection.
  $effect(() => {
    const ids = new Set(cards.map((c) => c.id));
    if ([...selected].some((id) => !ids.has(id))) selected = new Set([...selected].filter((id) => ids.has(id)));
  });

  // New rows arrive at the bottom; follow them unless the pointer is on the list.
  $effect(() => {
    void cards.length;
    if (body && !overList) body.scrollTop = body.scrollHeight;
  });

  function toggle(id: string) {
    const next = new Set(selected);
    if (!next.delete(id)) next.add(id);
    selected = next;
  }

  function dragState(active: boolean) {
    dragging = Math.max(0, dragging + (active ? 1 : -1));
    ondrag(active);
  }

</script>

<div class="list" data-list>
  <div class="header">{headerText(totals)}</div>
  {#if cards.length > 1}
    <DragAllBar
      cards={draggable(cards)}
      hotAction={barAction}
      {onclear}
      ondrag={dragState}
    />
  {/if}
  <div
    class="body"
    role="listbox"
    aria-multiselectable="true"
    aria-label={t('drop.list.aria')}
    style:max-height="{LIST_ROW * LIST_ROWS_MAX}px"
    bind:this={body}
    bind:clientHeight={viewport}
    onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
  >
    <div style:padding-top="{range.padTop}px" style:padding-bottom="{range.padBottom}px">
      {#each shown as card (card.id)}
        <ListRow
          {card}
          hovered={card.id === hovered}
          held={held || holding}
          selecting={selected.size > 0}
          checked={selected.has(card.id)}
          hotAction={hotAction?.card === card.id ? hotAction.action : null}
          {autoHideSecs}
          tool={tool?.id === card.id ? tool.kind : null}
          onclose={() => onclose(card.id)}
          ondrag={dragState}
          ontool={(kind) => ontool(card.id, kind)}
          ontoggle={() => toggle(card.id)}
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
  }
</style>
