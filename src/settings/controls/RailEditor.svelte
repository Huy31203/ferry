<script lang="ts">
  import { t } from '../../lib/i18n';
  import { store } from '../store.svelte';
  import { isMac } from '../../lib/ipc';
  import {
    DEFAULT_RAIL_TOOLS,
    MAX_TOOLS,
    addable,
    dropIndex,
    editorRows,
    knownCount,
    moveItem,
    toolLabel,
  } from '../../lib/rail';

  let {
    value,
    disabled,
    labelledby,
    onchange,
  }: {
    value: string[];
    disabled: boolean;
    labelledby: string;
    onchange: (tools: string[]) => void;
  } = $props();

  let list = $state<HTMLUListElement>();
  /** The row being dragged and the gap it would land in (0 to value.length). */
  let drag = $state<{ from: number; gap: number } | null>(null);

  const library = $derived(store.value?.pipelines.library ?? []); // stream F: pipelines can sit on the rail
  const rows = $derived(editorRows(value, library));
  const full = $derived(knownCount(value, library) >= MAX_TOOLS);
  const choices = $derived(addable(value, library));
  /** The index the dragged row would end at. The gap counts the row's own slot when it lies above. */
  const landing = $derived(drag === null ? null : drag.gap > drag.from ? drag.gap - 1 : drag.gap);
  /** The gap to mark, or null while the row would stay where it is. */
  const mark = $derived(drag !== null && landing !== drag.from ? drag.gap : null);

  function gapAt(y: number): number {
    const rects = Array.from(list?.children ?? []).map((row) => row.getBoundingClientRect());
    return dropIndex(
      y,
      rects.map((rect) => rect.top + rect.height / 2),
    );
  }

  // Pointer events on the handle: Tauri's file-drop handling swallows HTML5 drag and drop on Windows.
  function grab(event: PointerEvent, from: number) {
    if (disabled || event.button !== 0) return;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    drag = { from, gap: from };
  }

  function move(event: PointerEvent) {
    if (drag) drag = { ...drag, gap: gapAt(event.clientY) };
  }

  function drop() {
    const done = drag;
    drag = null;
    if (!done) return;
    const to = done.gap > done.from ? done.gap - 1 : done.gap;
    if (to !== done.from) onchange(moveItem(value, done.from, to));
  }

  function add(event: Event) {
    const select = event.currentTarget as HTMLSelectElement;
    const id = select.value;
    select.value = '';
    if (id) onchange([...value, id]);
  }
</script>

<div class="editor">
  {#if rows.length === 0}
    <p class="empty">{t('svc.rail.empty')}</p>
  {/if}
  <ul role="list" aria-labelledby={labelledby} bind:this={list}>
    {#each rows as row, i (row.id)}
      {@const name = toolLabel(row.id, isMac, library)}
      <li
        class:unknown={!row.known}
        class:before={mark === i}
        class:after={mark === rows.length && i === rows.length - 1}
        class:lifted={drag?.from === i}
      >
        <button
          type="button"
          class="handle"
          {disabled}
          aria-label={t('svc.rail.drag', { name })}
          onpointerdown={(event) => grab(event, i)}
          onpointermove={move}
          onpointerup={drop}
          onpointercancel={() => (drag = null)}
        >
          <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><circle cx="9" cy="6" r="1.6" /><circle cx="15" cy="6" r="1.6" /><circle cx="9" cy="12" r="1.6" /><circle cx="15" cy="12" r="1.6" /><circle cx="9" cy="18" r="1.6" /><circle cx="15" cy="18" r="1.6" /></svg>
        </button>
        <span class="name">{name}</span>
        <button
          type="button"
          class="step"
          disabled={disabled || i === 0}
          aria-label={t('svc.rail.move_up', { name })}
          onclick={() => onchange(moveItem(value, i, i - 1))}>↑</button
        >
        <button
          type="button"
          class="step"
          disabled={disabled || i === rows.length - 1}
          aria-label={t('svc.rail.move_down', { name })}
          onclick={() => onchange(moveItem(value, i, i + 1))}>↓</button
        >
        <button
          type="button"
          class="step"
          {disabled}
          aria-label={t('svc.rail.remove', { name })}
          onclick={() => onchange(value.filter((_, at) => at !== i))}>✕</button
        >
      </li>
    {/each}
  </ul>
  <div class="bar">
    <select
      aria-label={t('svc.rail.add_tool')}
      disabled={disabled || full || choices.length === 0}
      onchange={add}
    >
      <option value="">{t('svc.rail.add_tool')}</option>
      {#each choices as tool (tool.id)}
        <option value={tool.id}>{toolLabel(tool.id, isMac, library)}</option>
      {/each}
    </select>
    <button type="button" class="reset" {disabled} onclick={() => onchange([...DEFAULT_RAIL_TOOLS])}>
      {t('svc.rail.reset')}
    </button>
  </div>
  {#if full}
    <p class="hint" role="status">{t('svc.rail.full')}</p>
  {/if}
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    border: 1px solid var(--rail-line);
    border-radius: 8px;
  }
  ul:empty {
    display: none;
  }
  li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border-bottom: 1px solid var(--rail-line);
  }
  li:last-child {
    border-bottom: 0;
  }
  li.before {
    box-shadow: 0 -2px 0 var(--accent);
  }
  li.after {
    box-shadow: 0 2px 0 var(--accent);
  }
  li.lifted {
    opacity: 0.55;
  }
  li.unknown .name {
    color: var(--fg-2);
  }
  .name {
    flex: 1;
    min-width: 0;
    font-size: 13px;
  }
  button,
  select {
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  button {
    cursor: pointer;
  }
  .handle {
    padding: 4px;
    line-height: 0;
    border: 0;
    background: none;
    cursor: grab;
    touch-action: none;
  }
  .step {
    padding: 2px 8px;
  }
  .bar {
    display: flex;
    gap: 8px;
  }
  .reset {
    padding: 4px 12px;
  }
  button:disabled,
  select:disabled {
    opacity: 0.45;
    cursor: default;
  }
  button:focus-visible,
  select:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  .empty,
  .hint {
    margin: 0;
    font-size: 11px;
    color: var(--fg-2);
  }
</style>
