<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { startDrag } from '@crabnebula/tauri-plugin-drag';
  import { fanIcon } from '../lib/dragicon';
  import { t } from '../lib/i18n';
  import type { Settings } from '../lib/settings';
  import { COLLAPSE_MS, NO_DRAG, SUMMON_MS, countLabel, stripHeight, stripState, type DragView } from './strip';

  interface Entry { path: string; name: string; ext: string; is_dir: boolean }
  interface ShelfView { items: Entry[]; size: number; can_undo: boolean; transparency: number }

  let view = $state<ShelfView>({ items: [], size: 0, can_undo: false, transparency: 80 });
  let hover = $state(false);
  let summoned = $state(false);
  let summonTimer: ReturnType<typeof setTimeout> | undefined;
  let drag = $state<DragView>(NO_DRAG);
  let leaveTimer: ReturnType<typeof setTimeout> | undefined;

  const mode = $derived(stripState({ count: view.items.length, canUndo: view.can_undo, hover, summoned, drag }));
  const height = $derived(stripHeight(mode));
  const pile = $derived(view.items.slice(-3));

  $effect(() => {
    void invoke('shelf_layout', { height });
  });

  $effect(() => {
    const offs = [
      listen<ShelfView>('shelf:changed', (e) => (view = e.payload)),
      listen('shelf:summon', () => {
        summoned = true;
        clearTimeout(summonTimer);
        summonTimer = setTimeout(() => (summoned = false), SUMMON_MS);
      }),
      listen<DragView>('shelf:drag', (e) => (drag = e.payload)),
      listen<Settings>('settings:changed', (e) => (view.transparency = e.payload.floating.shelf_transparency)),
    ];
    void invoke<ShelfView>('shelf_view').then((v) => (view = v));
    return () => offs.forEach((off) => off.then((fn) => fn()));
  });

  // The panel is never key, so mouse enter/leave arrive unreliably: while it
  // shows, ask Rust where the pointer is.
  $effect(() => {
    if (mode === 'hidden') return;
    const id = setInterval(async () => {
      if (await invoke<boolean>('shelf_pointer_inside')) enter();
      else if (hover) leave();
    }, 100);
    return () => clearInterval(id);
  });


  const IMAGE = ['png', 'jpg', 'jpeg', 'webp', 'gif', 'heic', 'avif', 'tiff'];
  let icon = $state('');
  $effect(() => {
    const paths = view.items.filter((i) => IMAGE.includes(i.ext)).slice(-3).map((i) => i.path);
    let current = true;
    fanIcon(paths).then((url) => {
      if (current) icon = url;
    });
    return () => {
      current = false;
    };
  });

  async function dragPile(event: MouseEvent, paths = view.items.map((i) => i.path)) {
    if (event.button !== 0 || paths.length === 0) return;
    const keep = event.altKey;
    await invoke('shelf_drag_start');
    // A pile of folders or PDFs has no thumbnails: fall back to a 1×1 transparent PNG so the drag still starts.
    const blank = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
    startDrag({ item: paths, icon: icon || blank }, (result) => {
      void invoke('shelf_drag_finished', { paths, dropped: result.result === 'Dropped', keep });
    }).catch((e) => {
      console.warn('shelf drag', e);
      void invoke('shelf_drag_finished', { paths, dropped: false, keep: true });
    });
  }

  function enter() {
    clearTimeout(leaveTimer);
    leaveTimer = undefined;
    hover = true;
  }
  function leave() {
    if (leaveTimer) return;
    leaveTimer = setTimeout(() => {
      hover = false;
      leaveTimer = undefined;
    }, COLLAPSE_MS);
  }
</script>

<div
  class="shelf"
  class:open={mode === 'open' || mode === 'target'}
  class:target={mode === 'target'}
  style:--solid={1 - view.transparency / 100}
  role="region"
  aria-label={t('shelf.window_title')}
  onmouseenter={enter}
  onmouseleave={leave}
>
  <div class="grip"></div>
  {#if mode === 'resting' && view.items.length > 0}
    <span class="count">{view.items.length}</span>
  {/if}
  {#if mode === 'cleared'}
    <span class="note">{t('shelf.cleared')}</span>
  {/if}
  {#if mode === 'target'}
    <span class="drop">{t('shelf.drop')}</span>
  {:else if mode === 'open'}
    <div class="pile" role="presentation" onmousedown={(e) => dragPile(e)}>
      {#each pile as item, i (item.path)}
        <div class="doc" style:transform={`rotate(${(i - (pile.length - 1) / 2) * 7}deg)`}>
          <span>{item.is_dir ? '📁' : item.ext}</span>
        </div>
      {/each}
    </div>
    <span class="pill">{countLabel(view.items.length)}<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M2 3.5 5 6.5 8 3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg></span>
  {/if}
</div>

<style>
  :global(html), :global(body) { margin: 0; background: transparent; overflow: hidden; }
  .shelf {
    position: fixed; inset: 0;
    background: rgb(58 61 68 / var(--solid, 0.5)); border: 1px solid rgb(85 89 99 / var(--solid, 0.5)); border-bottom: 0;
    border-radius: 14px 14px 0 0; box-sizing: border-box; color: #fff;
    font: 12px -apple-system, system-ui, sans-serif; user-select: none;
  }
  .shelf.target { outline: 2px solid var(--accent, #0a84ff); outline-offset: -3px; }
  .grip { position: absolute; left: 50%; top: 6px; transform: translateX(-50%); width: 22px; height: 3px; border-radius: 2px; background: rgba(255, 255, 255, 0.75); }
  .count { position: absolute; right: 10px; top: 4px; font-size: 10px; font-weight: 700; }
  .note { position: absolute; left: 10px; top: 6px; font-size: 11px; }
  .drop { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; font-weight: 600; }
  .pile { position: absolute; left: 50%; top: 26px; width: 62px; height: 78px; transform: translateX(-50%); }
  .doc { position: absolute; inset: 0; background: #fff; border-radius: 3px; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4); display: flex; align-items: flex-end; justify-content: center; color: #555; font-size: 10px; text-transform: uppercase; padding-bottom: 6px; box-sizing: border-box; }
  .pill { position: absolute; left: 50%; bottom: 12px; transform: translateX(-50%); display: inline-flex; align-items: center; gap: 5px; background: rgba(0, 0, 0, 0.45); border-radius: 12px; padding: 3px 10px; white-space: nowrap; cursor: default; }
</style>
