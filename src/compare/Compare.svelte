<script lang="ts">
  import { onMount } from 'svelte';
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { t } from '../lib/i18n';
  import { isMac } from '../lib/ipc';
  import {
    clampPan,
    closesCompare,
    cssScale,
    dividerAt,
    nowBox,
    shotLabel,
    versioned,
    type CompareView,
    type Mode,
    type Shot,
    type Size,
    type Zoom,
  } from './view';

  let view = $state<CompareView | null>(null);
  let mode = $state<Mode>('slider');
  let zoom = $state<Zoom>('fit');
  let divider = $state(0.5);
  let pan = $state<Size>([0, 0]);
  let stageWidth = $state(0);
  let stageHeight = $state(0);
  let dpr = $state(window.devicePixelRatio || 1);
  let stage = $state<HTMLElement>();
  /** What a pointer drag moves, and where it started. */
  let drag: { what: 'divider' | 'pan'; x: number; y: number; from: Size } | null = null;

  const single = $derived(view !== null && view.original === null);
  const sides = $derived(mode === 'sides' && !single);
  /** Image-pixel boxes: the original's own size, the result's per `nowBox`. */
  const originalBox = $derived<Size>(dimsOf(view?.original ?? view?.now));
  const resultBox = $derived<Size>(
    view?.original ? nowBox(originalBox, dimsOf(view.now)) : dimsOf(view?.now),
  );
  /** The room one image has: the whole stage, or half of it side by side. */
  const pane = $derived<Size>([sides ? (stageWidth - 1) / 2 : stageWidth, stageHeight]);
  const scale = $derived(cssScale(zoom, originalBox, pane, dpr));
  const offset = $derived(clampPan(pan, [originalBox[0] * scale, originalBox[1] * scale], pane));

  function dimsOf(shot: Shot | undefined): Size {
    return shot?.dims ?? [1, 1];
  }

  function src(shot: Shot): string {
    return versioned(convertFileSrc(shot.path), shot);
  }

  /** CSS for an image of `box` image pixels, centred in its pane and panned. */
  function placed(box: Size): string {
    const width = box[0] * scale;
    const height = box[1] * scale;
    return `width:${width}px;height:${height}px;transform:translate(${offset[0] - width / 2}px,${offset[1] - height / 2}px)`;
  }

  function show(next: CompareView | null) {
    view = next;
    zoom = 'fit';
    divider = 0.5;
    pan = [0, 0];
  }

  function close() {
    show(null);
    void invoke('compare_close');
  }

  function onkeydown(event: KeyboardEvent) {
    if (closesCompare(event, isMac)) {
      event.preventDefault();
      close();
    }
  }

  function onpointerdown(event: PointerEvent) {
    if (!stage || event.button !== 0) return;
    const onDivider = (event.target as Element).closest('[data-divider]') !== null;
    drag = { what: onDivider ? 'divider' : 'pan', x: event.clientX, y: event.clientY, from: offset };
    stage.setPointerCapture(event.pointerId);
    if (onDivider) event.preventDefault();
  }

  function onpointermove(event: PointerEvent) {
    if (!drag || !stage) return;
    if (drag.what === 'divider') {
      const rect = stage.getBoundingClientRect();
      divider = dividerAt(event.clientX, rect.left, rect.width);
    } else {
      pan = [drag.from[0] + event.clientX - drag.x, drag.from[1] + event.clientY - drag.y];
    }
  }

  function onpointerup() {
    drag = null;
  }

  function setZoom(next: Zoom) {
    zoom = next;
    pan = [0, 0];
  }

  onMount(() => {
    // Listen first, then ask what to show, so nothing falls in between.
    const unlisten = Promise.all([
      listen<CompareView>('compare:show', (event) => show(event.payload)),
      listen('compare:hidden', () => show(null)),
    ]).then(async (stops) => {
      show(await invoke<CompareView | null>('compare_ready'));
      return stops;
    });
    // The window can move to a screen with another pixel ratio.
    const ratio = () => {
      dpr = window.devicePixelRatio || 1;
    };
    window.addEventListener('resize', ratio);
    return () => {
      window.removeEventListener('resize', ratio);
      unlisten.then((stops) => stops.forEach((stop) => stop()));
    };
  });
</script>

<svelte:window {onkeydown} />

{#if view}
  <div class="compare">
    <header class:single>
      {#if view.original}
        <span><b>{t('compare.original')}</b> {shotLabel(view.original)}</span>
        <span><b>{t('compare.now')}</b> {shotLabel(view.now, view.original.bytes)}</span>
      {:else}
        <span>{shotLabel(view.now)}</span>
      {/if}
    </header>

    <div
      class="stage"
      class:sides
      bind:this={stage}
      bind:clientWidth={stageWidth}
      bind:clientHeight={stageHeight}
      role="presentation"
      {onpointerdown}
      {onpointermove}
      {onpointerup}
      onpointercancel={onpointerup}
    >
      {#if view.original && sides}
        <div class="pane"><img src={src(view.original)} alt={t('compare.original')} style={placed(originalBox)} draggable="false" /></div>
        <div class="pane"><img src={src(view.now)} alt={t('compare.now')} style={placed(resultBox)} draggable="false" /></div>
      {:else if view.original}
        <div class="pane"><img src={src(view.original)} alt={t('compare.original')} style={placed(originalBox)} draggable="false" /></div>
        <div class="pane now" style="clip-path: inset(0 0 0 {divider * 100}%)">
          <img src={src(view.now)} alt={t('compare.now')} style={placed(resultBox)} draggable="false" />
        </div>
        <div
          class="divider"
          data-divider
          style="left: {divider * 100}%"
          role="slider"
          tabindex="0"
          aria-label={t('compare.divider')}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(divider * 100)}
          onkeydown={(event) => {
            if (event.key === 'ArrowLeft') divider = Math.max(0, divider - 0.05);
            if (event.key === 'ArrowRight') divider = Math.min(1, divider + 0.05);
          }}
        ><b>⇔</b></div>
        <span class="tag left">{t('compare.original')}</span>
        <span class="tag right">{t('compare.now')}</span>
      {:else}
        <div class="pane"><img src={src(view.now)} alt={view.name} style={placed(resultBox)} draggable="false" /></div>
      {/if}
    </div>

    <footer>
      {#if !single}
        <div class="seg" role="radiogroup" aria-label={t('compare.view')}>
          <button type="button" role="radio" aria-checked={mode === 'slider'} onclick={() => (mode = 'slider')}>{t('compare.slider')}</button>
          <button type="button" role="radio" aria-checked={mode === 'sides'} onclick={() => (mode = 'sides')}>{t('compare.sides')}</button>
        </div>
      {/if}
      <div class="seg" role="radiogroup" aria-label={t('compare.zoom')}>
        <button type="button" role="radio" aria-checked={zoom === 'fit'} onclick={() => setZoom('fit')}>{t('compare.fit')}</button>
        <button type="button" role="radio" aria-checked={zoom === 1} onclick={() => setZoom(1)}>{t('compare.zoom_100')}</button>
        <button type="button" role="radio" aria-checked={zoom === 2} onclick={() => setZoom(2)}>{t('compare.zoom_200')}</button>
      </div>
    </footer>
  </div>
{/if}

<style>
  .compare {
    display: flex;
    flex-direction: column;
    height: 100vh;
    user-select: none;
  }
  header,
  footer {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 10px;
    padding: 0 12px;
    background: var(--bg-group);
  }
  header {
    height: 36px;
    justify-content: space-between;
    border-bottom: 1px solid var(--rail-line);
  }
  header.single {
    justify-content: center;
  }
  footer {
    height: 48px;
    justify-content: center;
    border-top: 1px solid var(--rail-line);
  }
  .stage {
    position: relative;
    display: flex;
    flex: 1 1 auto;
    gap: 1px;
    overflow: hidden;
    cursor: grab;
    background: repeating-conic-gradient(#2a2a2a 0 25%, #222 0 50%) 0 0 / 16px 16px;
  }
  .pane {
    position: absolute;
    inset: 0;
    overflow: hidden;
  }
  .sides .pane {
    position: relative;
    flex: 1 1 0;
  }
  .pane img {
    position: absolute;
    top: 50%;
    left: 50%;
    max-width: none;
    image-rendering: auto;
  }
  .divider {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: #fff;
    cursor: ew-resize;
  }
  /* A wider grab area than the 2 px line. */
  .divider::before {
    position: absolute;
    inset: 0 -8px;
    content: '';
  }
  .divider b {
    position: absolute;
    top: 50%;
    left: -11px;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    margin-top: -12px;
    font-size: 11px;
    color: #111;
    background: #fff;
    border-radius: 50%;
  }
  .divider:focus-visible {
    outline: 2px solid var(--accent-text);
  }
  .tag {
    position: absolute;
    top: 10px;
    padding: 2px 8px;
    color: #fff;
    background: rgba(0, 0, 0, 0.6);
    border-radius: 7px;
    pointer-events: none;
  }
  .tag.left {
    left: 10px;
  }
  .tag.right {
    right: 10px;
  }
  .seg {
    display: flex;
    padding: 2px;
    background: var(--bg-content);
    border-radius: 7px;
  }
  .seg button {
    padding: 2px 10px;
    font: inherit;
    color: var(--fg);
    background: none;
    border: 0;
    border-radius: 5px;
  }
  .seg button[aria-checked='true'] {
    background: var(--bg-group);
    box-shadow: 0 0 0 1px var(--rail-line);
  }
  .seg button:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
</style>
