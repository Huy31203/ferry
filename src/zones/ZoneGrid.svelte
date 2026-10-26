<script lang="ts">
  import { t } from '../lib/i18n';
  import { tileState, type Media, type PerMedia } from '../lib/pipelines';
  import { zoneIcon } from '../lib/zone-icons';

  export interface Tile {
    id: number;
    name: string;
    icon: string;
    pipelines: PerMedia;
    captions: Partial<Record<Media, string>>;
  }

  let {
    tiles,
    dragged = [],
    hover = null,
    preview = false,
    onedit,
    ondone,
    onmenu,
  }: {
    tiles: Tile[];
    dragged?: Media[];
    /** The zone a drag is over. */
    hover?: number | null;
    /** The grid is shown from the editor: a Done tile follows Edit presets. */
    preview?: boolean;
    onedit?: () => void;
    ondone?: () => void;
    onmenu?: (zone: number, x: number, y: number) => void;
  } = $props();

  const states = $derived(tiles.map((tile) => tileState(tile, dragged)));
</script>

<div class="grid">
  {#each tiles as tile, i (tile.id)}
    {@const state = states[i]}
    <div
      class="tile"
      class:hover={hover === tile.id}
      class:dim={(hover !== null && hover !== tile.id) || (dragged.length > 0 && !state.enabled)}
      data-zone={tile.id}
      role="button"
      tabindex="-1"
      aria-label={`${tile.name}, ${state.caption}`}
      aria-disabled={!state.enabled}
      oncontextmenu={(event) => {
        event.preventDefault();
        onmenu?.(tile.id, event.clientX, event.clientY);
      }}
    >
      <svg aria-hidden="true" width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">{@html zoneIcon(tile.icon)}</svg>
      <strong>{tile.name}</strong>
      <span class="caption">{state.caption}</span>
    </div>
  {/each}
  <button type="button" class="tile action" onclick={onedit} disabled={!onedit}>
    <svg aria-hidden="true" width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">{@html zoneIcon('edit')}</svg>
    <strong>{t('pipe.zones.edit_presets')}</strong>
  </button>
  {#if preview}
    <button type="button" class="tile action" onclick={ondone} disabled={!ondone}>
      <svg aria-hidden="true" width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">{@html zoneIcon('done')}</svg>
      <strong>{t('pipe.zones.done')}</strong>
    </button>
  {/if}
</div>

<style>
  .grid {
    box-sizing: border-box;
    display: grid;
    grid-template-columns: repeat(4, 148px);
    grid-auto-rows: 100px;
    gap: 8px;
    padding: 12px;
    width: max-content;
  }
  .tile {
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    min-width: 0;
    padding: 8px;
    font: inherit;
    color: var(--rail-fg);
    text-align: center;
    background: var(--rail-bg);
    border: 2px solid var(--rail-line);
    border-radius: var(--card-radius);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
    transition:
      opacity var(--motion-in),
      transform 120ms;
  }
  .tile strong {
    max-width: 100%;
    overflow: hidden;
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .caption {
    max-width: 100%;
    overflow: hidden;
    font-size: 11px;
    color: var(--fg-2);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tile.hover {
    border-color: var(--accent);
    transform: scale(1.04);
  }
  .tile.dim {
    opacity: 0.5;
  }
  .action:not(:disabled) {
    cursor: pointer;
  }
  .action:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  @media (prefers-reduced-motion: reduce) {
    .tile {
      transition: none;
    }
    .tile.hover {
      transform: none;
    }
  }
</style>
