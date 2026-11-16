<script lang="ts">
  import { untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { startDrag } from '@crabnebula/tauri-plugin-drag';
  import { fanIcon } from '../lib/dragicon';
  import { formatBytes } from '../lib/format';
  import { t } from '../lib/i18n';
  import type { Settings } from '../lib/settings';
  import { COLLAPSE_MS, NO_DRAG, SUMMON_MS, countLabel, SLIDE_MS, stripHeight, stripState, stripWidth, WIDTH, type DragView } from './strip';

  interface Entry { path: string; name: string; ext: string; is_dir: boolean; size: number; card: boolean }
  interface ShelfView { items: Entry[]; size: number; can_undo: boolean; opacity: number; side: 'left' | 'right' }

  let view = $state<ShelfView>({ items: [], size: 0, can_undo: false, opacity: 95, side: 'right' });
  let hover = $state(false);
  let summoned = $state(false);
  let listing = $state(false);
  let menuOpen = $state(false);
  let summonTimer: ReturnType<typeof setTimeout> | undefined;
  let drag = $state<DragView>(NO_DRAG);
  let leaveTimer: ReturnType<typeof setTimeout> | undefined;

  const mode = $derived(stripState({ count: view.items.length, canUndo: view.can_undo, hover: hover || listing, summoned, drag }));
  const height = $derived(stripHeight(mode, listing));
  const pile = $derived(view.items.slice(-3));

  const width = $derived(stripWidth(mode, listing));
  // The panel is a fixed-size box that slides (a transform, so it stays smooth). The window
  // grows first and shrinks only after the slide, so a collapse plays out.
  let panelH = $state(0); // the visible height
  let boxH = $state(0); // the window's height
  let panelW = $state(WIDTH);
  // Resizing the window changes `lift` without moving the panel; that step must not animate.
  let animate = $state(true);
  let shrinkTimer: ReturnType<typeof setTimeout> | undefined;
  const frames = (n: number, fn: () => void) =>
    requestAnimationFrame(() => (n <= 1 ? fn() : frames(n - 1, fn)));
  $effect(() => {
    const [h, w] = [height, width];
    const bh = untrack(() => boxH);
    clearTimeout(shrinkTimer);
    if (h <= bh) {
      // Slide first; the window follows once the slide is over.
      panelH = h;
      panelW = w;
      void invoke('shelf_layout', { height: h, width: w });
      if (h < bh) {
        shrinkTimer = setTimeout(() => {
          animate = false;
          boxH = h;
          frames(2, () => (animate = true));
        }, SLIDE_MS + 20);
      }
    } else {
      void invoke('shelf_layout', { height: h, width: w }).then(() => {
        animate = false;
        boxH = h;
        panelW = w;
        frames(2, () => {
          animate = true;
          frames(1, () => (panelH = h));
        });
      });
    }
  });

  $effect(() => {
    const offs = [
      listen<ShelfView>('shelf:changed', (e) => (view = e.payload)),
      listen('shelf:show_items', () => (listing = true)),
      listen('shelf:summon', () => {
        summoned = true;
        clearTimeout(summonTimer);
        summonTimer = setTimeout(() => (summoned = false), SUMMON_MS);
      }),
      listen<DragView>('shelf:drag', (e) => (drag = e.payload)),
      listen<Settings>('settings:changed', (e) => (view.opacity = e.payload.floating.shelf_opacity)),
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
      else if (hover && !menuOpen) leave();
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

  const BLANK = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';

  /** A drag starts only once the pointer has moved: a click must not flash the drag image. */
  function armDrag(event: MouseEvent, paths = view.items.map((i) => i.path)) {
    if (event.button !== 0 || paths.length === 0) return;
    const keep = event.altKey;
    const [x0, y0] = [event.screenX, event.screenY];
    const move = (e: MouseEvent) => {
      if (Math.hypot(e.screenX - x0, e.screenY - y0) < 5) return;
      done();
      void beginDrag(paths, keep);
    };
    const done = () => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', done);
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', done);
  }

  async function beginDrag(paths: string[], keep: boolean) {
    await invoke('shelf_drag_start');
    // One row drags its own icon; a pile of folders or PDFs has no thumbnail, so a 1×1 PNG keeps the drag starting.
    const one = paths.length === 1 ? view.items.find((i) => i.path === paths[0]) : undefined;
    const image = one ? (IMAGE.includes(one.ext) ? await fanIcon([one.path]) : '') : icon;
    startDrag({ item: paths, icon: image || BLANK }, (result) => {
      void invoke('shelf_drag_finished', { paths, dropped: result.result === 'Dropped', keep });
    }).catch((e) => {
      console.warn('shelf drag', e);
      void invoke('shelf_drag_finished', { paths, dropped: false, keep: true });
    });
  }

  async function openMenu(e: MouseEvent) {
    menuOpen = true;
    try {
      await invoke('shelf_menu', { x: e.clientX, y: e.clientY });
    } finally {
      menuOpen = false;
    }
  }
  function itemMenu(e: MouseEvent, path: string) {
    e.preventDefault();
    void invoke('shelf_item_menu', { path, x: e.clientX, y: e.clientY });
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
      listing = false;
      leaveTimer = undefined;
    }, COLLAPSE_MS);
  }
</script>

<div
  class="shelf"
  class:open={mode === 'open' || mode === 'target'}
  class:target={mode === 'target'}
  class:left={view.side === 'left'}
  class:right={view.side === 'right'}
  class:still={!animate}
  style:--solid={view.opacity / 100}
  style:--w="{panelW}px"
  style:--box="{boxH}px"
  style:--lift="{boxH - panelH}px"
  role="region"
  aria-label={t('shelf.window_title')}
  onmouseenter={enter}
  onmouseleave={leave}
>
  {#if mode !== 'cleared'}
    <div class="grip"></div>
  {/if}
  {#if mode === 'resting' && view.items.length > 0}
    <span class="count">{view.items.length}</span>
  {/if}
  {#if mode === 'cleared'}
    <div class="cleared">
      <span class="note">{t('shelf.cleared')}</span>
      <button class="undo" onclick={() => invoke('shelf_undo')}>{t('shelf.undo')}</button>
    </div>
  {/if}
  {#if mode === 'target'}
    <span class="drop">{t('shelf.drop')}</span>
  {:else if mode === 'open' && listing}
    <ul class="list">
      {#each view.items as item (item.path)}
        <li role="presentation" onmousedown={(e) => armDrag(e, [item.path])} oncontextmenu={(e) => itemMenu(e, item.path)}>
          <span class="badge">{item.is_dir ? '📁' : item.ext || '·'}</span>
          <span class="meta">
            <span class="name">{item.name}</span>
            <span class="sub">{item.is_dir ? t('shelf.folder') : formatBytes(item.size)}</span>
          </span>
          <span class="acts">
            {#if item.card}
              <button type="button" title={t('shelf.restore')} aria-label={t('shelf.restore')} onmousedown={(e) => e.stopPropagation()} onclick={() => invoke('shelf_restore', { path: item.path })}>
                <svg width="11" height="11" viewBox="0 0 12 12" aria-hidden="true"><path d="M4.5 2.5 2 5l2.5 2.5M2 5h5a3 3 0 0 1 0 6H5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
              </button>
            {/if}
            <button type="button" title={t('shelf.menu.reveal')} aria-label={t('shelf.menu.reveal')} onmousedown={(e) => e.stopPropagation()} onclick={() => invoke('shelf_reveal', { path: item.path })}>
              <svg width="11" height="11" viewBox="0 0 12 12" aria-hidden="true"><path d="M4 2h6v6M10 2 3 9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
            </button>
            <button type="button" title={t('shelf.menu.remove')} aria-label={t('shelf.menu.remove')} onmousedown={(e) => e.stopPropagation()} onclick={() => invoke('shelf_remove', { paths: [item.path] })}>
              <svg width="11" height="11" viewBox="0 0 12 12" aria-hidden="true"><path d="M2.5 6h7" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
            </button>
          </span>
        </li>
      {/each}
    </ul>
    <button class="pill" onclick={openMenu}>{countLabel(view.items.length)}<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M2 3.5 5 6.5 8 3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg></button>
  {:else if mode === 'open'}
    <div class="pile" role="presentation" onmousedown={(e) => armDrag(e)}>
      {#each pile as item, i (item.path)}
        <div class="doc" style:transform={`rotate(${(i - (pile.length - 1) / 2) * 7}deg)`}>
          <span>{item.is_dir ? '📁' : item.ext}</span>
        </div>
      {/each}
    </div>
    <button class="pill" onclick={openMenu}>{countLabel(view.items.length)}<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M2 3.5 5 6.5 8 3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg></button>
  {/if}
</div>

<style>
  :global(html), :global(body) { margin: 0; background: transparent; overflow: hidden; }
  .shelf {
    position: fixed; bottom: 0; width: var(--w); height: var(--box);
    transform: translateY(var(--lift));
    transition: transform 340ms cubic-bezier(0.22, 0.8, 0.3, 1), width 340ms cubic-bezier(0.22, 0.8, 0.3, 1);
    will-change: transform;
    background: rgb(58 61 68 / var(--solid, 0.5)); border: 1px solid rgb(85 89 99 / var(--solid, 0.5)); border-bottom: 0;
    border-radius: 14px 14px 0 0; box-sizing: border-box; color: #fff;
    font: 12px -apple-system, system-ui, sans-serif; user-select: none;
  }
  .shelf.right { right: 0; }
  .shelf.left { left: 0; }
  .shelf.still { transition: none; }
  .shelf.target { outline: 2px solid var(--accent, #0a84ff); outline-offset: -3px; }
  .grip { position: absolute; left: 50%; top: 6px; transform: translateX(-50%); width: 22px; height: 3px; border-radius: 2px; background: rgba(255, 255, 255, 0.75); }
  .count { position: absolute; right: 10px; top: 4px; font-size: 10px; font-weight: 700; }
  .cleared { position: absolute; left: 0; right: 0; top: 0; height: 27px; display: flex; align-items: center; justify-content: space-between; padding: 0 12px; }
  .note { font-size: 11px; }
  .drop { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; font-weight: 600; }
  .pile { position: absolute; left: 50%; top: 26px; width: 62px; height: 78px; transform: translateX(-50%); }
  .doc { position: absolute; inset: 0; background: #fff; border-radius: 3px; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4); display: flex; align-items: flex-end; justify-content: center; color: #555; font-size: 10px; text-transform: uppercase; padding-bottom: 6px; box-sizing: border-box; }
  .undo { background: none; border: 0; color: var(--accent, #58a6ff); font: inherit; font-size: 11px; font-weight: 600; cursor: pointer; padding: 0; }
  .list { position: absolute; left: 8px; right: 8px; top: 12px; bottom: 44px; margin: 0; padding: 0; list-style: none; overflow-y: auto; }
  .list li { display: flex; align-items: center; gap: 8px; padding: 4px 6px; border-radius: 6px; cursor: grab; }
  .list li:hover { background: rgba(255, 255, 255, 0.12); }
  .badge { flex: none; width: 28px; height: 28px; border-radius: 5px; background: #fff; color: #555; font-size: 9px; text-transform: uppercase; display: flex; align-items: center; justify-content: center; overflow: hidden; }
  .meta { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .name { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sub { font-size: 10px; opacity: 0.65; }
  .acts { flex: none; display: none; gap: 2px; }
  .list li:hover .acts { display: flex; }
  .acts button { width: 20px; height: 20px; padding: 0; border: 0; border-radius: 5px; background: rgba(0, 0, 0, 0.35); color: inherit; display: flex; align-items: center; justify-content: center; cursor: pointer; }
  .acts button:hover { background: rgba(0, 0, 0, 0.6); }
  .pill { border: 0; color: inherit; font: inherit; position: absolute; left: 50%; bottom: 12px; transform: translateX(-50%); display: inline-flex; align-items: center; gap: 5px; background: rgba(0, 0, 0, 0.45); border-radius: 12px; padding: 3px 10px; white-space: nowrap; cursor: default; }
</style>
