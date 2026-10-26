<script lang="ts">
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { startDrag } from '@crabnebula/tauri-plugin-drag';
  import { fade } from 'svelte/transition';
  import { autoHides, canDrag, isBusy, needsOriginal, thumbSrc, railActions, type RailAction, type Tool } from '../lib/cards';
  import {
    SCALE_STOPS,
    // stream B: audio (the Compression button drives bitrate on audio cards)
    compressionLabel,
    compressionPatch,
    compressionRound,
    compressionStart,
    compressionStops,
    nextCompression,
    formatChoices,
    frameBox,
    nextScale,
    scaleLabel,
    scalePatch,
    sliderIndex,
    stopYs,
    sliderTrack,
    labelledStops,
    stopIndex,
    type CropFrame,
    type Quality,
  } from '../lib/edits';
  import { formatBytes, savingPercent } from '../lib/format';
  import { t, type StringKey } from '../lib/i18n';
  import { isMac, type EditsPatch, type Format, type JobUpdate } from '../lib/ipc';
  import { DPI_STOPS, currentDpi, dpiLabel, dpiPatch, nextDpi } from '../lib/pdf'; // sE pdf-formats
  import { pdfConfig } from '../lib/pdfConfig.svelte'; // sE pdf-formats
  import { DEFAULT_RAIL_TOOLS, MAX_TOOLS, addable, knownCount, pipelineFor, toolLabel, type RailPipeline } from '../lib/rail';
  import { extLabel, formatDuration, isVideo, ringDash } from '../lib/video'; // stream A: video
  import FormatBar from './FormatBar.svelte';
  import Slider from './Slider.svelte';

  let {
    card,
    hovered = false,
    compact = false,
    railRight = false,
    held = false,
    hotAction = null,
    autoHideSecs,
    compression,
    railTools = DEFAULT_RAIL_TOOLS,
    railPipelines = [],
    tool = null,
    frame = null,
    onclose,
    ondrag,
    ontool,
  }: {
    card: JobUpdate;
    hovered?: boolean;
    compact?: boolean;
    railRight?: boolean;
    /** The stack holds every card open (pointer on the Drag all bar, or a drag in progress). */
    held?: boolean;
    /** The `data-action` of the button the pointer is on, if it is on this card. */
    hotAction?: string | null;
    autoHideSecs: number;
    /** The settings' compression quality: where the Compression slider starts on a card without its own. */
    compression: number;
    /** svc: the tool ids the rail shows, in the user's order. */
    railTools?: string[];
    /** stream F: the library pipelines a `pipeline:<id>` tool runs. */
    railPipelines?: RailPipeline[];
    /** This card's open tool, if any. */
    tool?: Tool | null;
    /** The crop popover's live frame, drawn over the thumbnail. */
    frame?: CropFrame | null;
    onclose: () => void;
    ondrag: (active: boolean) => void;
    /** Open a tool on this card, or close it with null. */
    ontool: (kind: Tool | null) => void;
  } = $props();

  let focused = $state(false);
  let thumb = $state<HTMLImageElement>();
  let cardEl = $state<HTMLElement>();
  let copied = $state<'ok' | 'failed' | null>(null);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;
  let tip = $state<{ text: string; left: number; top: number; room: number } | null>(null);
  let tipWidth = $state(0);
  let renameError = $state<string | null>(null);

  const busy = $derived(isBusy(card.state));
  const notice = $derived(card.name === '' && card.ext === '');
  const fileName = $derived(card.ext ? `${card.name}.${card.ext}` : card.name);
  const rail = $derived(railActions(card, compact, railTools, railPipelines));
  const canRename = $derived(canDrag(card));

  const frameRect = $derived(
    frame && card.dims_before
      ? frameBox(frame, card.dims_before, compact ? [220, 150] : [300, 200])
      : null,
  );
  // The format bar replaces the detail line while the card is hovered (spec §5.4).
  const showFormats = $derived(hovered && rail.length > 0 && card.state.kind !== 'restored');
  const stateLabel = $derived(t(`card.state.${card.state.kind}` as StringKey));
  const label = $derived(
    t('card.aria', { name: notice ? t('card.notice') : fileName, state: stateLabel }),
  );
  const motionOut = window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 150;

  // Finished cards fade out after the configured time; hover or focus holds them (§5.5).
  $effect(() => {
    if (!autoHides(card.state) || autoHideSecs === 0 || hovered || focused || held || tool !== null) return;
    const timer = setTimeout(onclose, autoHideSecs * 1000);
    return () => clearTimeout(timer);
  });

  function act(command: string, args: Record<string, unknown> = {}): Promise<boolean> {
    return invoke(command, { id: card.id, ...args }).then(
      () => true,
      (e) => {
        console.warn(command, e);
        return false;
      },
    );
  }

  // Copy gives no other sign that it worked, so the button says so for a moment.
  async function copy() {
    const ok = await act('card_copy');
    copied = ok ? 'ok' : 'failed';
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copied = null), 1400);
  }

  $effect(() => {
    if (tool !== 'rename') renameError = null;
  });

  function focusSelect(input: HTMLInputElement) {
    input.focus();
    input.select();
  }

  async function rename(stem: string) {
    if (stem === card.name) {
      ontool(null);
      return;
    }
    try {
      await invoke('card_rename', { id: card.id, stem });
      ontool(null);
    } catch (e) {
      renameError = String(e);
    }
  }

  function renameKey(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      void rename((event.currentTarget as HTMLInputElement).value);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      ontool(null);
    }
  }

  /** The slider's track length in px: the longest that keeps its panel (live line, track, labels) under the rail and inside the card. */
  const TRACK = $derived(sliderTrack(compact ? 150 : 200));
  /** Fallback switch (Task 11): false if drags never reach the panel; the buttons then only step. */
  const PRESS_AND_DRAG = true;
  type SliderKind = 'downscale' | 'compression' | 'dpi'; // 'dpi': sE pdf-formats
  let slide = $state<{
    kind: SliderKind;
    startY: number;
    start: number;
    index: number;
    moved: boolean;
    left: number;
  } | null>(null);
  const quality = $derived(compressionStart(card, compression)); // stream B: audio
  const qualityStops = $derived(compressionStops(card));
  const dpi = $derived(currentDpi(card, pdfConfig)); // sE pdf-formats
  /** The stops the open slider steps through. */
  const stopsOf = (kind: SliderKind): Quality[] =>
    kind === 'downscale' ? SCALE_STOPS : kind === 'dpi' ? DPI_STOPS : qualityStops;
  const slideLabel = $derived.by(() => {
    if (!slide) return '';
    if (slide.kind === 'dpi') return dpiLabel(DPI_STOPS[slide.index]);
    return slide.kind === 'downscale'
      ? scaleLabel(SCALE_STOPS[slide.index], card.dims_before, card.edits)
      : compressionLabel(card, qualityStops[slide.index]);
  });

  /** Every stop's short value, such as "50 %" or "q 60", for the slider's labels. */
  const slideStops = $derived(
    slide?.kind === 'dpi'
      ? DPI_STOPS.map(dpiLabel)
      : slide?.kind === 'downscale'
        ? SCALE_STOPS.map((scale) => scaleLabel(scale, null, card.edits))
        : qualityStops.map((stop) => compressionLabel(card, stop)),
  );

  /** Every stop's distance from the track's top, px: the track is the value axis. */
  const slideYs = $derived.by(() => {
    return stopYs(stopsOf(slide?.kind ?? 'compression'), TRACK);
  });

  /** The stops that get a value label (round ones, spaced for the track). */
  const slideLabelled = $derived(
    slide?.kind === 'dpi'
      ? labelledStops(DPI_STOPS, TRACK, () => true)
      : slide?.kind === 'downscale'
        ? labelledStops(SCALE_STOPS, TRACK)
        : labelledStops(qualityStops, TRACK, compressionRound(card)),
  );

  function edit(edits: EditsPatch) {
    void act('card_edit', { edits });
  }

  function step(kind: SliderKind) {
    if (kind === 'dpi') {
      const next = nextDpi(dpi);
      if (next !== null) edit(dpiPatch(next));
      return;
    }
    edit(kind === 'downscale' ? scalePatch(nextScale(card.edits.scale)) : compressionPatch(card, nextCompression(card, quality)));
  }

  // Press and hold: a drag of a few px turns the press into a slider; a plain click is one step.
  function press(kind: SliderKind, event: PointerEvent) {
    if (!PRESS_AND_DRAG || event.button !== 0 || !card.can_edit) return;
    const button = event.currentTarget as HTMLElement;
    button.setPointerCapture(event.pointerId);
    const box = cardEl?.getBoundingClientRect();
    const rect = button.getBoundingClientRect();
    const start =
      kind === 'dpi'
        ? stopIndex(DPI_STOPS, dpi)
        : kind === 'downscale'
          ? stopIndex(SCALE_STOPS, card.edits.scale ?? 100)
          : stopIndex(qualityStops, quality);
    slide = {
      kind,
      startY: event.clientY,
      start,
      index: start,
      moved: false,
      left: rect.left + rect.width / 2 - (box?.left ?? 0),
    };
  }

  function slideMove(event: PointerEvent) {
    if (!slide) return;
    const dy = event.clientY - slide.startY;
    if (!slide.moved && Math.abs(dy) < 4) return;
    if (!slide.moved) ontool('slider');
    const stops = stopsOf(slide.kind);
    slide = { ...slide, moved: true, index: sliderIndex(stops, slide.start, dy, TRACK) };
  }

  // Release applies; releasing back at the start value changes nothing.
  function release() {
    const done = slide;
    slide = null;
    if (!done) return;
    if (!done.moved) {
      step(done.kind);
      return;
    }
    ontool(null);
    if (done.index === done.start) return;
    edit(
      done.kind === 'dpi'
        ? dpiPatch(DPI_STOPS[done.index])
        : done.kind === 'downscale'
          ? scalePatch(SCALE_STOPS[done.index])
          : compressionPatch(card, qualityStops[done.index]),
    );
  }

  function cancelSlide() {
    if (slide?.moved) ontool(null);
    slide = null;
  }

  // Enter or Space on a focused button: one step (a pointer press is handled by `release`).
  function keyStep(kind: SliderKind, event: MouseEvent) {
    if ((event.detail === 0 || !PRESS_AND_DRAG) && card.can_edit) step(kind);
  }


  function convert(format: Format, keepBoth: boolean) {
    void act('card_convert', { format, keepBoth });
  }

  function formatTip(format: string): string {
    if (!card.can_edit) return card.edit_hint ?? t('card.no_original');
    const choice = formatChoices(card).find((c) => c.format === format);
    if (!choice) return '';
    if (choice.firstFrame) return t('card.first_frame');
    if (choice.firstPage) return t('pdf.first_page');
    return t(isMac ? 'format.tip.mac' : 'format.tip.windows', { format: choice.label });
  }


  function tipText(action: string): string {
    if (action.startsWith('format-')) return formatTip(action.slice('format-'.length));
    if (needsOriginal(action as RailAction) && !card.can_edit) return card.edit_hint ?? t('card.no_original');
    if (pipelineFor(action, railPipelines)) return toolLabel(action, isMac, railPipelines);
    switch (action) {
      case 'downscale':
        return card.audio ? t('audio.card.cover_downscale') : t('card.downscale'); // stream B: audio
      case 'dpi':
        return t('pdf.dpi.tip');
      case 'compression':
        return card.audio ? t('audio.card.bitrate') : t('card.compression');
      case 'crop':
        return t('card.crop');
      case 'compare':
        return t('card.compare');
      case 'rename':
        return t('card.rename');
      case 'copy':
        return copied === 'ok' ? t('card.copied') : copied === 'failed' ? t('card.copy_failed') : t('card.copy');
      case 'restore':
        return t('card.restore');
      case 'reveal':
      case 'aggressive':
      case 'preview':
      case 'fit':
        return toolLabel(action, isMac);
      case 'add':
        return t('svc.rail.add');
      case 'more':
        return t('card.menu');
      case 'close':
        return t(busy ? 'card.cancel' : 'card.close');
      case 'retry':
        return t('card.retry.tip');
      case 'details':
        return t('card.details.tip');
      case 'reoptimise':
        return t('card.reoptimise.tip');
      default:
        return '';
    }
  }

  // The tooltip sits under the hot button, kept inside the card.
  $effect(() => {
    const action = hotAction;
    void copied;
    // The button may come or go with the card's state (a busy card has no toolbar).
    void rail.length;
    void card.state.kind;
    const button = action && cardEl?.querySelector<HTMLElement>(`[data-action="${action}"]`);
    if (!action || !button || !cardEl) {
      tip = null;
      return;
    }
    const box = cardEl.getBoundingClientRect();
    const rect = button.getBoundingClientRect();
    // Under the button, or above it near the bottom edge (the format bar) so the card does not clip it.
    const below = rect.bottom - box.top + 6;
    tip = {
      text: tipText(action),
      left: Math.max(8, rect.left - box.left),
      top: below + 24 > box.height ? rect.top - box.top - 28 : below,
      room: box.width,
    };
  });

  // The ⋯ button opens the native menu just under itself; right-click opens it at the pointer.
  function openMenu(event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    void act('card_menu', { x: rect.left, y: rect.bottom + 4 });
  }

  // svc: the + button lists the tools that are not on the rail; full means none can be added
  function openAddMenu(event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const full = knownCount(railTools, railPipelines) >= MAX_TOOLS;
    const items = addable(railTools, railPipelines).map((tool) => ({
      id: tool.id,
      label: toolLabel(tool.id, isMac, railPipelines),
      enabled: !full,
    }));
    invoke('svc_rail_menu', { x: rect.left, y: rect.bottom + 4, items }).catch((e) =>
      console.warn('svc_rail_menu', e),
    );
  }

  function close() {
    if (busy) act('card_cancel');
    onclose();
  }

  // A drag preview the size of the card's thumbnail, not of the full image.
  function dragIcon(): string {
    const canvas = document.createElement('canvas');
    const width = 120;
    canvas.width = width;
    canvas.height = 80;
    const ctx = canvas.getContext('2d');
    if (ctx && thumb?.complete && thumb.naturalWidth > 0) {
      const scale = Math.min(width / thumb.naturalWidth, 80 / thumb.naturalHeight);
      const w = thumb.naturalWidth * scale;
      const h = thumb.naturalHeight * scale;
      ctx.drawImage(thumb, (width - w) / 2, (80 - h) / 2, w, h);
    }
    return canvas.toDataURL('image/png');
  }

  function drag(event: MouseEvent) {
    if (event.button !== 0 || !canDrag(card)) return;
    if ((event.target as HTMLElement).closest('button')) return;
    ondrag(true);
    startDrag({ item: [card.output as string], icon: dragIcon() })
      .catch((e) => console.warn('drag', e))
      .finally(() => ondrag(false));
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === 'Escape' && slide) cancelSlide();
  }}
/>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<article
  class="card"
  class:hovered
  class:compact
  class:busy
  class:error={card.state.kind === 'error'}
  class:rail-right={railRight}
  data-card-id={card.id}
  bind:this={cardEl}
  aria-label={label}
  onmousedown={drag}
  oncontextmenu={(event) => {
    event.preventDefault();
    void act('card_menu', { x: event.clientX, y: event.clientY });
  }}
  onfocusin={() => (focused = true)}
  onfocusout={() => (focused = false)}
  out:fade={{ duration: motionOut }}
>
  <div class="thumb" class:document={card.source_format === 'pdf'} aria-hidden="true">
    {#if thumbSrc(card)}
      <!-- sE pdf-formats: a poster stands in for originals the webview cannot draw -->
      <img bind:this={thumb} src={convertFileSrc(thumbSrc(card)!)} alt="" crossorigin="anonymous" />
    {:else if card.audio}
      <!-- stream B: audio -->
      <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2 13a2 2 0 0 0 2-2V7a2 2 0 0 1 4 0v13a2 2 0 0 0 4 0V4a2 2 0 0 1 4 0v13a2 2 0 0 0 4 0v-4a2 2 0 0 1 2-2" /></svg>
    {:else if isVideo(card.source_format ?? card.format)}
      <!-- stream A: video -->
      <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2" /><path d="M7 3v18" /><path d="M3 7.5h4" /><path d="M3 12h18" /><path d="M3 16.5h4" /><path d="M17 3v18" /><path d="M17 7.5h4" /><path d="M17 16.5h4" /></svg>
    {:else}
      <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" /><circle cx="9" cy="9" r="2" /><path d="m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21" /></svg>
    {/if}
  </div>
  {#if card.video}
    <!-- stream A: video -->
    <span class="duration" aria-label={t('video.card.duration', { time: formatDuration(card.video.duration_ms) })}>{formatDuration(card.video.duration_ms)}</span>
  {/if}
  {#if card.pages !== null && card.pages > 1}
    <!-- sE pdf-formats -->
    <span class="pages" aria-hidden="true">{t('pdf.badge.pages', { n: card.pages })}</span>
  {/if}
  {#if busy || card.state.kind === 'error'}
    <div class="dim" aria-hidden="true"></div>
  {/if}
  {#if busy}
    <svg class="ring" class:fixed={card.state.kind === 'optimising' && card.state.progress > 0} width="56" height="56" viewBox="0 0 56 56" aria-hidden="true">
      <circle cx="28" cy="28" r="24" fill="none" stroke="rgba(255,255,255,.25)" stroke-width="4" />
      <!-- stream A: video (a video fills the ring with its progress and does not spin) -->
      <circle
        cx="28"
        cy="28"
        r="24"
        fill="none"
        stroke="#fff"
        stroke-width="4"
        stroke-linecap="round"
        stroke-dasharray={card.state.kind === 'optimising' && card.state.progress > 0 ? ringDash(card.state.progress) : '40 151'}
        transform={card.state.kind === 'optimising' && card.state.progress > 0 ? 'rotate(-90 28 28)' : undefined}
      />
    </svg>
  {/if}
  <div class="scrim" aria-hidden="true"></div>
  {#if frameRect}
    <div
      class="frame"
      aria-hidden="true"
      style="left: {frameRect.left}px; top: {frameRect.top}px; width: {frameRect.width}px; height: {frameRect.height}px"
    ></div>
  {/if}

  {#if rail.length > 0}
    <!-- svc: one block per action, drawn in the user's order so Tab follows what is shown -->
    <div class="rail">
      {#each rail as action (action)}
      {#if action === 'downscale'}
        <button
          type="button"
          data-action="downscale"
          class:hot={hotAction === 'downscale'}
          aria-label={tipText('downscale')}
          disabled={!card.can_edit}
          onpointerdown={(event) => press('downscale', event)}
          onpointermove={slideMove}
          onpointerup={release}
          onpointercancel={cancelSlide}
          onclick={(event) => keyStep('downscale', event)}
        >
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 14h6v6" /><path d="M20 10h-6V4" /><path d="m14 10 7-7" /><path d="m3 21 7-7" /></svg>
        </button>
      {:else if action === 'dpi'}
        <!-- sE pdf-formats -->
        <button
          type="button"
          data-action="dpi"
          class:hot={hotAction === 'dpi'}
          aria-label={tipText('dpi')}
          disabled={!card.can_edit}
          onpointerdown={(event) => press('dpi', event)}
          onpointermove={slideMove}
          onpointerup={release}
          onpointercancel={cancelSlide}
          onclick={(event) => keyStep('dpi', event)}
        >
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" /><path d="M14 2v6h6" /><path d="M9 15h6" /></svg>
        </button>
      {:else if action === 'compression'}
        <button
          type="button"
          data-action="compression"
          class:hot={hotAction === 'compression'}
          aria-label={tipText('compression')}
          disabled={!card.can_edit}
          onpointerdown={(event) => press('compression', event)}
          onpointermove={slideMove}
          onpointerup={release}
          onpointercancel={cancelSlide}
          onclick={(event) => keyStep('compression', event)}
        >
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9" /><path d="M12 3a9 9 0 0 1 0 18z" fill="currentColor" /></svg>
        </button>
      {:else if action === 'crop'}
        <button type="button" data-action="crop" class:hot={hotAction === 'crop'} aria-label={tipText('crop')} disabled={!card.can_edit} onclick={() => ontool(tool === 'crop' ? null : 'crop')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 2v14a2 2 0 0 0 2 2h14" /><path d="M18 22V8a2 2 0 0 0-2-2H2" /></svg>
        </button>
      {:else if action === 'restore'}
        <button type="button" data-action="restore" class:hot={hotAction === 'restore'} aria-label={tipText('restore')} onclick={() => act('card_restore')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 3-6.7L3 8" /><path d="M3 3v5h5" /></svg>
        </button>
      {:else if action === 'compare'}
        <button type="button" data-action="compare" class:hot={hotAction === 'compare'} aria-label={tipText('compare')} disabled={!card.can_edit} onclick={() => act('open_compare')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" /><path d="M12 3v18" /></svg>
        </button>
      {:else if action === 'copy'}
        <button
          type="button"
          data-action="copy"
          class:hot={hotAction === 'copy'}
          class:ok={copied === 'ok'}
          class:failed={copied === 'failed'}
          aria-label={tipText('copy')}
          onclick={copy}
        >
          {#if copied === 'ok'}
            <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg>
          {:else}
            <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" /></svg>
          {/if}
        </button>
      {:else if action === 'reveal'}
        <button type="button" data-action="reveal" class:hot={hotAction === 'reveal'} aria-label={tipText('reveal')} onclick={() => act('card_reveal')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2" /></svg>
        </button>
      {:else if action === 'aggressive'}
        <button type="button" data-action="aggressive" class:hot={hotAction === 'aggressive'} aria-label={tipText('aggressive')} disabled={!card.can_edit} onclick={() => act('card_rerun', { aggressive: true })}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 14a1 1 0 0 1-.78-1.63l9.9-10.2a.5.5 0 0 1 .86.46l-1.92 6.02A1 1 0 0 0 13 10h7a1 1 0 0 1 .78 1.63l-9.9 10.2a.5.5 0 0 1-.86-.46l1.92-6.02A1 1 0 0 0 11 14z" /></svg>
        </button>
      {:else if action === 'rename'}
        <button type="button" data-action="rename" class:hot={hotAction === 'rename'} aria-label={tipText('rename')} onclick={() => ontool('rename')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z" /><path d="m15 5 4 4" /></svg>
        </button>
      {:else if action === 'preview'}
        <button type="button" data-action="preview" class:hot={hotAction === 'preview'} aria-label={tipText('preview')} onclick={() => act('quick_preview')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0" /><circle cx="12" cy="12" r="3" /></svg>
        </button>
      {:else if action === 'fit'}
        <button type="button" data-action="fit" class:hot={hotAction === 'fit'} aria-label={tipText('fit')} disabled={!card.can_edit} onclick={() => ontool(tool === 'fit' ? null : 'fit')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z" /><path d="M14 2v5a1 1 0 0 0 1 1h5" /><path d="M12 18v-6" /><path d="m9 15 3 3 3-3" /></svg>
        </button>
      {:else if pipelineFor(action, railPipelines)}
        <!-- stream F: a pipeline from the library, run on this card's original -->
        <button type="button" data-action={action} class:hot={hotAction === action} aria-label={tipText(action)} disabled={!card.can_edit} onclick={() => act('pipe_run', { pipeline: pipelineFor(action, railPipelines)?.id })}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="2.5" /><circle cx="18" cy="12" r="2.5" /><circle cx="6" cy="18" r="2.5" /><path d="M8.5 6H13a3 3 0 0 1 3 3v.5M8.5 18H13a3 3 0 0 0 3-3v-.5" /></svg>
        </button>
      {:else if action === 'more'}
        <button type="button" data-action="more" class:hot={hotAction === 'more'} aria-label={tipText('more')} onclick={(event) => openMenu(event)}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><circle cx="5" cy="12" r="1.8" /><circle cx="12" cy="12" r="1.8" /><circle cx="19" cy="12" r="1.8" /></svg>
        </button>
      {/if}
      {/each}
      {#if !compact}
        <button type="button" data-action="add" class:hot={hotAction === 'add'} aria-label={tipText('add')} onclick={openAddMenu}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14" /><path d="M12 5v14" /></svg>
        </button>
      {/if}
    </div>
  {/if}
  {#if slide?.moved}
    <Slider
      index={slide.index}
      ys={slideYs}
      label={slideLabel}
      stops={slideStops}
      labelled={slideLabelled}
      left={slide.left}
      track={TRACK}
      width={compact ? 220 : 300}
    />
  {/if}

  <button class="close" class:hot={hotAction === 'close'} type="button" data-action="close" aria-label={tipText('close')} onclick={close}>
    <svg aria-hidden="true" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12" /></svg>
  </button>

  <div class="info">
    {#if !notice}
      <div class="chips">
        {#if tool === 'rename'}
          <input
            class="name name-field"
            type="text"
            value={card.name}
            aria-label={t('card.rename_named', { name: fileName })}
            aria-invalid={renameError !== null}
            use:focusSelect
            onkeydown={renameKey}
            onblur={() => ontool(null)}
          />
        {:else if canRename}
          <button
            type="button"
            class="name"
            data-action="rename"
            class:hot={hotAction === 'rename'}
            aria-label={t('card.rename_named', { name: fileName })}
            onclick={() => ontool('rename')}>{card.name}</button
          >
        {:else}
          <span class="name">{card.name}</span>
        {/if}
        {#if card.ext}<span class="ext">{extLabel(card)}</span>{/if}
        {#if card.converted}<span class="converted">{t('card.converted')}</span>{/if}
      </div>
      {#if renameError}
        <span class="rename-error" role="alert">{renameError}</span>
      {/if}
    {/if}
    <div class="line">
      {#if card.state.kind === 'queued'}
        <span>{t('card.waiting')} {formatBytes(card.before)}</span>
      {:else if card.state.kind === 'optimising'}
        <span>{card.state.step}…</span>
      {:else if card.state.kind === 'done'}
        <span>{formatBytes(card.before)} → {formatBytes(card.after ?? card.before)}</span>
        <span class="pill ok">
          <svg aria-hidden="true" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg>
          {savingPercent(card.before, card.after ?? card.before)}
        </span>
      {:else if card.state.kind === 'already_optimal' || card.state.kind === 'restored'}
        <span>{formatBytes(card.before)}</span>
        <span class="pill grey">
          <svg aria-hidden="true" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"><path d="M5 12h14" /></svg>
          {stateLabel}
        </span>
      {:else if card.state.kind === 'larger'}
        <span>{formatBytes(card.before)}</span>
        <span class="pill amber">
          <svg aria-hidden="true" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"><path d="M12 19V5M5 12l7-7 7 7" /></svg>
          {stateLabel}
        </span>
      {:else if card.state.kind === 'error'}
        <span class="reason" role="alert">{card.state.reason}</span>
      {/if}
    </div>
    {#if card.note}
      <span class="note" role="status">{card.note}</span>
    {/if}
    {#if card.state.kind === 'error'}
      <div class="buttons">
        {#if card.can_retry}
          <button type="button" data-action="retry" class:hot={hotAction === 'retry'} onclick={() => act('card_rerun', { aggressive: false })}>{t('card.retry')}</button>
        {/if}
        <button type="button" data-action="details" class:hot={hotAction === 'details'} onclick={() => invoke('open_log')}>{t('card.details')}</button>
      </div>
    {:else if card.state.kind === 'restored'}
      <div class="buttons">
        <button type="button" data-action="reoptimise" class:hot={hotAction === 'reoptimise'} onclick={() => act('card_rerun', { aggressive: false })}>{t('card.reoptimise')}</button>
      </div>
    {:else if showFormats}
      <FormatBar {card} {hotAction} onconvert={convert} />
    {:else if card.state.kind === 'done' && card.detail && !compact}
      <span class="detail">{card.detail}</span>
    {/if}
  </div>
  {#if tip && !slide?.moved}
    <!-- Left edge on the button's, pulled left so the whole tip stays 6 px inside the card. -->
    <div
      class="tip"
      role="tooltip"
      style="left: {Math.max(6, Math.min(tip.left, tip.room - tipWidth - 6))}px; top: {tip.top}px"
      bind:clientWidth={tipWidth}
    >{tip.text}</div>
  {/if}
</article>

<style>
  .card {
    position: relative;
    width: 300px;
    height: 200px;
    border-radius: var(--card-radius);
    overflow: hidden;
    background: #2c2c2e;
    color: #fff;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.22), 0 0 0 0.5px rgba(0, 0, 0, 0.18);
    font-variant-numeric: tabular-nums;
    animation: slide-in var(--motion-in) ease-out;
  }
  .card.compact {
    width: 220px;
    height: 150px;
  }
  .card.error {
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.22), inset 0 0 0 2px #ff453a;
  }
  .thumb {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding-bottom: 36px;
    box-sizing: border-box;
    color: rgba(255, 255, 255, 0.55);
  }
  /* The whole image is shown, with black bars where its shape differs from the card's. */
  .thumb:has(img) {
    background: #000;
    padding-bottom: 0;
  }
  /* A white page against black bars reads as a second frame inside the card's edge. */
  .thumb.document:has(img) {
    background: #2c2c2e;
  }
  .thumb img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .dim {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
  }
  .ring {
    position: absolute;
    left: calc(50% - 28px);
    top: calc(50% - 46px);
    animation: spin 1s linear infinite;
  }
  /* stream A: video. The length sits top-left; the rail takes that corner while the card is hovered. */
  .duration,
  .pages {
    position: absolute;
    left: 8px;
    top: 8px;
    padding: 1px 6px;
    border-radius: 999px;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    font-size: 11px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    transition: opacity 120ms;
  }
  .card:not(.rail-right):hover .duration,
  .card:not(.rail-right).hovered .duration,
  .card:not(.rail-right):focus-within .duration,
  .card:not(.rail-right):hover .pages,
  .card:not(.rail-right).hovered .pages,
  .card:not(.rail-right):focus-within .pages {
    opacity: 0;
  }
  .scrim {
    position: absolute;
    inset: auto 0 0 0;
    height: 50%;
    background: linear-gradient(to top, rgba(0, 0, 0, 0.78), rgba(0, 0, 0, 0.4) 50%, rgba(0, 0, 0, 0));
  }
  .rail {
    position: absolute;
    left: 8px;
    top: 8px;
    display: flex;
    flex-direction: row;
    gap: 2px;
    padding: 3px;
    border-radius: 10px;
    background: var(--rail-bg);
    color: var(--rail-fg);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25), 0 0 0 0.5px var(--rail-line);
    opacity: 0;
    visibility: hidden;
    transition: opacity 120ms, visibility 120ms;
  }
  .rail-right .rail {
    left: auto;
    right: 40px;
  }
  .rail button {
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: inherit;
    display: grid;
    place-items: center;
    padding: 0;
    cursor: pointer;
  }
  .rail button:hover:not(:disabled),
  .rail button.hot:not(:disabled) {
    background: var(--rail-line);
  }
  .rail button:active:not(:disabled) {
    transform: scale(0.92);
  }
  .rail button.ok {
    background: rgba(48, 209, 88, 0.28);
    color: #1f9d46;
  }
  .rail button.failed {
    background: rgba(255, 69, 58, 0.25);
    color: var(--error);
  }
  .rail button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .close:hover,
  .close.hot {
    background: rgba(0, 0, 0, 0.8);
    border-color: rgba(255, 255, 255, 0.7);
  }
  .close:active {
    transform: scale(0.92);
  }
  .close {
    position: absolute;
    top: 8px;
    right: 8px;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 0.5px solid rgba(255, 255, 255, 0.35);
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    display: grid;
    place-items: center;
    padding: 0;
    cursor: pointer;
    opacity: 0;
    visibility: hidden;
    transition: opacity 120ms, visibility 120ms;
  }
  .card:hover .rail,
  .card.hovered .rail,
  .card:focus-within .rail,
  .card:hover .close,
  .card.hovered .close,
  .card:focus-within .close {
    opacity: 1;
    visibility: visible;
  }
  .info {
    position: absolute;
    left: 12px;
    right: 12px;
    bottom: 10px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .chips {
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }
  /* Both chips are 20px tall with their text centred, so "png" lines up with the name. */
  .name,
  .ext {
    display: inline-flex;
    align-items: center;
    box-sizing: border-box;
    height: 20px;
    line-height: 1;
    border-radius: 999px;
  }
  .name {
    min-width: 0;
    flex-shrink: 1;
    background: rgba(255, 255, 255, 0.16);
    font-size: 12px;
    padding: 0 8px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  button.name {
    border: 0;
    color: inherit;
    font: inherit;
    font-size: 12px;
    cursor: text;
  }
  button.name:hover,
  button.name.hot {
    background: rgba(255, 255, 255, 0.28);
  }
  .name-field {
    width: 180px;
    border: 1px solid rgba(255, 255, 255, 0.6);
    color: #fff;
    font: inherit;
    font-size: 12px;
    outline: none;
  }
  .name-field[aria-invalid='true'] {
    border-color: #ff453a;
  }
  .frame {
    position: absolute;
    box-sizing: border-box;
    border: 2px dashed #fff;
    box-shadow: 0 0 0 999px rgba(0, 0, 0, 0.45);
    pointer-events: none;
  }
  .note {
    font-size: 11px;
    font-weight: 600;
    color: #ffd27a;
  }
  .converted {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.82);
  }
  .rename-error {
    font-size: 11px;
    color: #ffb3ae;
  }
  .ext {
    flex-shrink: 0;
    border: 1px solid rgba(255, 255, 255, 0.45);
    font-size: 11px;
    font-weight: 600;
    /* The font's line box sits low in a 1-line-height pill; 3px below lifts the x-height onto the centre. */
    padding: 0 7px 3px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 600;
    min-width: 0;
  }
  .reason {
    color: #ffb3ae;
    font-weight: 500;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 12px;
    font-weight: 600;
    padding: 1px 8px 1px 6px;
    border-radius: 999px;
    white-space: nowrap;
  }
  .pill.ok {
    background: rgba(48, 209, 88, 0.26);
    color: #8cf5ae;
  }
  .pill.amber {
    background: rgba(255, 159, 10, 0.28);
    color: #ffd27a;
  }
  .pill.grey {
    background: rgba(255, 255, 255, 0.2);
    color: #f2f2f7;
  }
  .buttons {
    display: flex;
    gap: 6px;
  }
  .buttons button:hover,
  .buttons button.hot {
    background: rgba(255, 255, 255, 0.3);
  }
  .buttons button:active {
    transform: scale(0.96);
  }
  .buttons button {
    border: 0.5px solid rgba(255, 255, 255, 0.4);
    background: rgba(255, 255, 255, 0.16);
    color: #fff;
    font: inherit;
    font-size: 12px;
    padding: 2px 10px;
    border-radius: 999px;
    cursor: pointer;
  }
  .tip {
    position: absolute;
    z-index: 5;
    max-width: 240px;
    padding: 4px 8px;
    border-radius: 6px;
    background: rgba(20, 20, 22, 0.94);
    color: #fff;
    font-size: 11px;
    font-weight: 500;
    line-height: 1.3;
    white-space: nowrap;
    pointer-events: none;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4), 0 0 0 0.5px rgba(255, 255, 255, 0.15);
  }
  .detail {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.82);
  }
  @keyframes slide-in {
    from {
      transform: translateX(24px);
      opacity: 0;
    }
    to {
      transform: none;
      opacity: 1;
    }
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  /* stream A: video */
  .ring.fixed {
    animation: none;
  }
  @media (prefers-reduced-motion: reduce) {
    .card,
    .ring {
      animation: none;
    }
  }
</style>
