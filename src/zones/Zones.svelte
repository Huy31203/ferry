<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { onMount } from 'svelte';
  import { tileState, type Media } from '../lib/pipelines';
  import ZoneGrid, { type Tile } from './ZoneGrid.svelte';

  interface ZonesView {
    tiles: Tile[];
    preview: boolean;
    dragged: Media[];
  }

  let view = $state<ZonesView>({ tiles: [], preview: false, dragged: [] });
  /** What the drag now over the grid carries; the grid's own report until one enters. */
  let carried = $state<Media[]>([]);
  let hover = $state<number | null>(null);

  const dragged = $derived(carried.length > 0 ? carried : view.dragged);

  /** The enabled tile under a drag position (physical px), if any. */
  function zoneAt(position: { x: number; y: number }): number | null {
    const found = document.elementFromPoint(position.x / window.devicePixelRatio, position.y / window.devicePixelRatio);
    const id = Number((found?.closest('[data-zone]') as HTMLElement | null)?.dataset.zone);
    const tile = view.tiles.find((t) => t.id === id);
    return tile && tileState(tile, dragged).enabled ? id : null;
  }

  const unique = (media: (Media | null)[]): Media[] => [...new Set(media.filter((m): m is Media => m !== null))];

  function openEditor() {
    void invoke('pipe_open_editor', { pipeline: null, tab: 'zones' });
    void invoke('pipe_zones_hide');
  }

  onMount(() => {
    // Listen first, then ask what to show, so nothing falls in between.
    const unlistenView = listen<ZonesView>('pipe:zones', (event) => (view = event.payload)).then(async (stop) => {
      view = await invoke<ZonesView>('pipe_zones_ready');
      return stop;
    });
    const unlistenDrop = getCurrentWebview().onDragDropEvent(async (event) => {
      const payload = event.payload;
      if (payload.type === 'enter') {
        void invoke('pipe_zones_drag', { entered: true });
        carried = unique(await invoke<(Media | null)[]>('pipe_classify', { paths: payload.paths }));
      } else if (payload.type === 'over') {
        hover = zoneAt(payload.position);
      } else if (payload.type === 'drop') {
        const zone = zoneAt(payload.position);
        hover = null;
        carried = [];
        if (zone !== null) void invoke('pipe_zone_drop', { zone, paths: payload.paths });
        else void invoke('pipe_zones_drag', { entered: false });
      } else {
        hover = null;
        carried = [];
        void invoke('pipe_zones_drag', { entered: false });
      }
    });
    return () => {
      void unlistenView.then((stop) => stop());
      void unlistenDrop.then((stop) => stop());
    };
  });
</script>

<ZoneGrid
  tiles={view.tiles}
  {dragged}
  {hover}
  preview={view.preview}
  onedit={openEditor}
  ondone={() => invoke('pipe_zones_hide')}
  onmenu={(zone, x, y) => invoke('pipe_zone_menu', { zone, x, y })}
/>

<style>
  :global(html),
  :global(body) {
    background: transparent;
    overflow: hidden;
    user-select: none;
  }
</style>
