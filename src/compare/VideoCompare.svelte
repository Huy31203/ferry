<script lang="ts">
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { t } from '../lib/i18n';
  import { drifted, followerRate, followerTime, frameStep, ratio } from './video';
  import { shotLabel, versioned, type CompareView, type Shot } from './view';

  // stream A: video. The original leads; the result follows at the original's pace.
  let { view }: { view: CompareView } = $props();

  let lead = $state<HTMLVideoElement>();
  let follow = $state<HTMLVideoElement>();
  let playing = $state(false);
  let time = $state(0);
  let swapped = $state(false);
  let failed = $state({ original: false, now: false });

  const single = $derived(view.original === null);
  /** The player that sets the time: the original, or the result when it is alone. */
  const leader = $derived(single ? view.now : (view.original as Shot));
  const r = $derived(ratio(view.original?.duration_ms ?? null, view.now.duration_ms));
  const fps = $derived(view.now.fps);
  const duration = $derived((leader.duration_ms ?? 0) / 1000);

  function src(shot: Shot): string {
    return versioned(convertFileSrc(shot.path), shot);
  }

  function sync() {
    if (!lead || !follow || single || failed.original || failed.now) return;
    const want = followerTime(lead.currentTime, r);
    if (drifted(follow.currentTime, want, fps)) follow.currentTime = want;
  }

  function seek(to: number) {
    if (single) {
      if (follow) follow.currentTime = to;
    } else {
      if (lead && !failed.original) lead.currentTime = to;
      if (follow && !failed.now) follow.currentTime = followerTime(to, r);
    }
    time = to;
  }

  function toggle() {
    const players = [lead, follow].filter((p): p is HTMLVideoElement => p !== undefined);
    if (players.length === 0) return;
    if (playing) {
      for (const p of players) p.pause();
      playing = false;
      return;
    }
    if (follow && !single) follow.playbackRate = followerRate(r);
    for (const p of players) void p.play().catch(() => {});
    playing = true;
  }

  function step(dir: 1 | -1) {
    if (playing) toggle();
    const clock = single ? follow : lead;
    seek(frameStep(clock?.currentTime ?? time, fps, dir, clock?.duration || duration));
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'k') {
      event.preventDefault();
      toggle();
    } else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      event.preventDefault();
      step(event.key === 'ArrowRight' ? 1 : -1);
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="compare">
  <header class:single>
    {#if view.original}
      <span><b>{t('compare.original')}</b> {shotLabel(view.original)}</span>
      <span><b>{t('compare.now')}</b> {shotLabel(view.now, view.original.bytes)}</span>
    {:else}
      <span>{shotLabel(view.now)}</span>
    {/if}
  </header>

  <div class="stage" class:swapped>
    {#if view.original}
      <div class="pane original">
        {#if failed.original}
          <p class="cant">{t('video.compare.cant_play', { format: view.original.format })}</p>
        {:else}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video
            bind:this={lead}
            src={src(view.original)}
            preload="auto"
            playsinline
            muted
            ontimeupdate={() => {
              time = lead?.currentTime ?? 0;
              sync();
            }}
            onseeked={sync}
            onended={() => (playing = false)}
            onerror={() => (failed.original = true)}
          ></video>
        {/if}
        <span class="tag">{t('compare.original')}</span>
      </div>
    {/if}
    <div class="pane">
      {#if failed.now}
        <p class="cant">{t('video.compare.cant_play', { format: view.now.format })}</p>
      {:else}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video
          bind:this={follow}
          src={src(view.now)}
          preload="auto"
          playsinline
          muted
          ontimeupdate={() => {
            if (single) time = follow?.currentTime ?? 0;
          }}
          onended={() => {
            if (single) playing = false;
          }}
          onerror={() => (failed.now = true)}
        ></video>
      {/if}
      {#if !single}<span class="tag">{t('compare.now')}</span>{/if}
    </div>
  </div>

  <div class="scrub">
    <input
      type="range"
      min="0"
      max={duration}
      step="0.01"
      value={time}
      aria-label={t('video.compare.position')}
      oninput={(event) => seek(Number(event.currentTarget.value))}
    />
  </div>
  <footer>
    <button type="button" aria-label={t('video.compare.back')} onclick={() => step(-1)}>◀</button>
    <button type="button" aria-label={playing ? t('video.compare.pause') : t('video.compare.play')} onclick={toggle}>
      {playing ? '❚❚' : '▶'}
    </button>
    <button type="button" aria-label={t('video.compare.forward')} onclick={() => step(1)}>▶▏</button>
    {#if !single}
      <button type="button" class="swap" aria-pressed={swapped} onclick={() => (swapped = !swapped)}>{t('video.compare.swap')}</button>
    {/if}
  </footer>
</div>

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
  .scrub {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    height: 40px;
    padding: 0 12px;
    background: var(--bg-group);
  }
  .scrub input {
    width: 100%;
  }
  .stage {
    display: flex;
    flex: 1 1 auto;
    gap: 1px;
    overflow: hidden;
    background: #000;
  }
  .stage.swapped {
    flex-direction: row-reverse;
  }
  .pane {
    position: relative;
    flex: 1 1 0;
    min-width: 0;
    display: grid;
    place-items: center;
  }
  video {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .cant {
    padding: 0 16px;
    text-align: center;
    color: #fff;
  }
  .tag {
    position: absolute;
    top: 10px;
    left: 10px;
    padding: 2px 8px;
    color: #fff;
    background: rgba(0, 0, 0, 0.6);
    border-radius: 7px;
    pointer-events: none;
  }
  button {
    padding: 2px 12px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 7px;
    cursor: pointer;
  }
  button[aria-pressed='true'] {
    background: var(--bg-group);
    box-shadow: 0 0 0 1px var(--accent-text);
  }
  button:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
</style>
