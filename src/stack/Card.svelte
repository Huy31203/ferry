<script lang="ts">
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { startDrag } from '@crabnebula/tauri-plugin-drag';
  import { fade } from 'svelte/transition';
  import { autoHides, canDrag, isBusy, railActions } from '../lib/cards';
  import { formatBytes, savingPercent } from '../lib/format';
  import { t, type StringKey } from '../lib/i18n';
  import { isMac, type JobUpdate } from '../lib/ipc';

  let {
    card,
    hovered = false,
    compact = false,
    railRight = false,
    autoHideSecs,
    onclose,
  }: {
    card: JobUpdate;
    hovered?: boolean;
    compact?: boolean;
    railRight?: boolean;
    autoHideSecs: number;
    onclose: () => void;
  } = $props();

  let focused = $state(false);
  let thumb = $state<HTMLImageElement>();

  const busy = $derived(isBusy(card.state));
  const notice = $derived(card.name === '' && card.ext === '');
  const fileName = $derived(card.ext ? `${card.name}.${card.ext}` : card.name);
  const rail = $derived(railActions(card));
  const stateLabel = $derived(t(`card.state.${card.state.kind}` as StringKey));
  const label = $derived(
    t('card.aria', { name: notice ? t('card.notice') : fileName, state: stateLabel }),
  );
  const motionOut = window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 150;

  // Finished cards fade out after the configured time; hover or focus holds them (§5.5).
  $effect(() => {
    if (!autoHides(card.state) || autoHideSecs === 0 || hovered || focused) return;
    const timer = setTimeout(onclose, autoHideSecs * 1000);
    return () => clearTimeout(timer);
  });

  function act(command: string, args: Record<string, unknown> = {}) {
    invoke(command, { id: card.id, ...args }).catch((e) => console.warn(command, e));
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
      const scale = Math.max(width / thumb.naturalWidth, 80 / thumb.naturalHeight);
      const w = thumb.naturalWidth * scale;
      const h = thumb.naturalHeight * scale;
      ctx.drawImage(thumb, (width - w) / 2, (80 - h) / 2, w, h);
    }
    return canvas.toDataURL('image/png');
  }

  function drag(event: MouseEvent) {
    if (event.button !== 0 || !canDrag(card)) return;
    if ((event.target as HTMLElement).closest('button')) return;
    startDrag({ item: [card.output as string], icon: dragIcon() }).catch((e) =>
      console.warn('drag', e),
    );
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<article
  class="card"
  class:hovered
  class:compact
  class:busy
  class:error={card.state.kind === 'error'}
  class:rail-right={railRight}
  data-card-id={card.id}
  aria-label={label}
  onmousedown={drag}
  onfocusin={() => (focused = true)}
  onfocusout={() => (focused = false)}
  out:fade={{ duration: motionOut }}
>
  <div class="thumb" aria-hidden="true">
    {#if card.thumb_path}
      <img bind:this={thumb} src={convertFileSrc(card.thumb_path)} alt="" crossorigin="anonymous" />
    {:else}
      <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" /><circle cx="9" cy="9" r="2" /><path d="m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21" /></svg>
    {/if}
  </div>
  {#if busy || card.state.kind === 'error'}
    <div class="dim" aria-hidden="true"></div>
  {/if}
  {#if busy}
    <svg class="ring" width="56" height="56" viewBox="0 0 56 56" aria-hidden="true">
      <circle cx="28" cy="28" r="24" fill="none" stroke="rgba(255,255,255,.25)" stroke-width="4" />
      <circle cx="28" cy="28" r="24" fill="none" stroke="#fff" stroke-width="4" stroke-linecap="round" stroke-dasharray="40 151" />
    </svg>
  {/if}
  <div class="scrim" aria-hidden="true"></div>

  {#if rail.length > 0}
    <div class="rail">
      {#if rail.includes('copy')}
        <button type="button" aria-label={t('card.copy')} onclick={() => act('card_copy')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" /></svg>
        </button>
      {/if}
      {#if rail.includes('reveal')}
        <button type="button" aria-label={t(isMac ? 'card.reveal.mac' : 'card.reveal.windows')} onclick={() => act('card_reveal')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>
        </button>
      {/if}
      {#if rail.includes('restore')}
        <button type="button" aria-label={t('card.restore')} onclick={() => act('card_restore')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 3-6.7L3 8" /><path d="M3 3v5h5" /></svg>
        </button>
      {/if}
      {#if rail.includes('aggressive')}
        <button type="button" aria-label={t('card.aggressive')} onclick={() => act('card_rerun', { aggressive: true })}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M13 2 3 14h9l-1 8 10-12h-9l1-8z" /></svg>
        </button>
      {/if}
      {#if !compact}
        <button type="button" aria-label={t('card.more')} disabled>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
        </button>
      {/if}
    </div>
  {/if}

  <button class="close" type="button" aria-label={t(busy ? 'card.cancel' : 'card.close')} onclick={close}>
    <svg aria-hidden="true" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12" /></svg>
  </button>

  <div class="info">
    {#if !notice}
      <div class="chips">
        <span class="name">{card.name}</span>
        {#if card.ext}<span class="ext">{card.ext}</span>{/if}
      </div>
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
    {#if card.state.kind === 'error'}
      <div class="buttons">
        {#if card.can_retry}
          <button type="button" onclick={() => act('card_rerun', { aggressive: false })}>{t('card.retry')}</button>
        {/if}
        <button type="button" onclick={() => invoke('open_log')}>{t('card.details')}</button>
      </div>
    {:else if card.state.kind === 'restored'}
      <div class="buttons">
        <button type="button" onclick={() => act('card_rerun', { aggressive: false })}>{t('card.reoptimise')}</button>
      </div>
    {:else if card.state.kind === 'done' && card.detail && !compact}
      <span class="detail">{card.detail}</span>
    {/if}
  </div>
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
  .thumb img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
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
  .scrim {
    position: absolute;
    inset: auto 0 0 0;
    height: 64%;
    background: linear-gradient(to top, rgba(0, 0, 0, 0.8), rgba(0, 0, 0, 0.5) 45%, rgba(0, 0, 0, 0));
  }
  .rail {
    position: absolute;
    left: 8px;
    top: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 4px;
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
    right: 8px;
    top: 40px;
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
  .rail button:hover:not(:disabled) {
    background: var(--rail-line);
  }
  .rail button:disabled {
    opacity: 0.45;
    cursor: default;
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
    transition: left 120ms, right 120ms;
  }
  .card:not(.rail-right):is(:hover, .hovered, :focus-within):has(.rail) .info {
    left: 52px;
  }
  .card.rail-right:is(:hover, .hovered, :focus-within):has(.rail) .info {
    right: 52px;
  }
  .chips {
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }
  .name {
    min-width: 0;
    flex-shrink: 1;
    background: rgba(255, 255, 255, 0.16);
    font-size: 12px;
    padding: 2px 8px;
    border-radius: 999px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ext {
    flex-shrink: 0;
    border: 1px solid rgba(255, 255, 255, 0.45);
    font-size: 11px;
    font-weight: 600;
    padding: 1px 7px;
    border-radius: 999px;
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
  @media (prefers-reduced-motion: reduce) {
    .card,
    .ring {
      animation: none;
    }
  }
</style>
