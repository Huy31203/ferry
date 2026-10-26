<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { onMount } from 'svelte';
  import { formatBytes } from '../lib/format';
  import { t } from '../lib/i18n';
  import type { Pipeline, StepReport } from '../lib/pipelines';

  let {
    pipeline,
    blocked,
  }: {
    pipeline: Pipeline;
    /** The pipeline has problems, or is not an image pipeline. */
    blocked: boolean;
  } = $props();

  let well: HTMLElement | undefined;
  let hot = $state(false);
  let running = $state(false);
  let reports = $state<StepReport[] | null>(null);
  let failure = $state<string | null>(null);

  const imageOnly = $derived(pipeline.media !== 'image');
  const firstError = $derived(reports?.find((r) => r.error !== null) ?? null);

  async function test(path: string) {
    if (blocked || imageOnly || running) return;
    running = true;
    failure = null;
    reports = null;
    try {
      reports = await invoke<StepReport[]>('pipe_test', { pipeline: $state.snapshot(pipeline), path });
    } catch (e) {
      failure = String(e);
    } finally {
      running = false;
    }
  }

  async function choose() {
    try {
      const path = await invoke<string | null>('pipe_pick_file');
      if (path) await test(path);
    } catch (e) {
      failure = String(e);
    }
  }

  /** Whether a drag event's position (physical px) is over the well. */
  function inside(position: { x: number; y: number }): boolean {
    if (!well) return false;
    const box = well.getBoundingClientRect();
    const [x, y] = [position.x / window.devicePixelRatio, position.y / window.devicePixelRatio];
    return x >= box.left && x <= box.right && y >= box.top && y <= box.bottom;
  }

  onMount(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === 'leave') hot = false;
      else if (payload.type === 'over') hot = inside(payload.position);
      else if (payload.type === 'drop') {
        hot = false;
        if (inside(payload.position) && payload.paths[0]) void test(payload.paths[0]);
      }
    });
    return () => void unlisten.then((off) => off());
  });
</script>

<section>
  <h3>{t('pipe.test.title')}</h3>
  <div bind:this={well} class="well" class:hot>
    {#if running}
      <p>{t('pipe.test.running')}</p>
    {:else}
      <p>{t('pipe.test.drop')}</p>
      <button type="button" disabled={blocked || imageOnly} onclick={choose}>{t('pipe.test.choose')}</button>
    {/if}
  </div>
  {#if imageOnly}
    <p class="note">{t('pipe.test.image_only')}</p>
  {:else if blocked}
    <p class="note">{t('pipe.test.disabled')}</p>
  {/if}
  {#if failure}
    <p class="error" role="alert">{failure}</p>
  {/if}
  {#if reports}
    <p class="summary" class:bad={firstError} role="status">
      {firstError ? t('pipe.test.failed', { n: firstError.step + 1 }) : t('pipe.test.passed')}
    </p>
    <ol>
      {#each reports as report (report.step)}
        <li>
          <span class="n">{report.step + 1}</span>
          <span class="body">
            <span>{report.caption}</span>
            <span class="meta">
              {#if report.bytes !== null}{formatBytes(report.bytes)}{/if}
              {#if report.dims}· {report.dims[0]}×{report.dims[1]}{/if}
              · {t('pipe.test.ms', { ms: report.millis })}
            </span>
            {#if report.note}<span class="meta">{report.note}</span>{/if}
            {#if report.error}<span class="error">{report.error}</span>{/if}
          </span>
        </li>
      {/each}
    </ol>
    <button type="button" class="reveal" onclick={() => invoke('pipe_reveal_test')}>{t('pipe.test.reveal')}</button>
  {/if}
</section>

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h3 {
    margin: 0;
    font-size: 13px;
  }
  .well {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 14px 8px;
    color: var(--fg-2);
    text-align: center;
    border: 1px dashed var(--rail-line);
    border-radius: 8px;
  }
  .well.hot {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .well p,
  .note,
  .summary {
    margin: 0;
  }
  .note {
    font-size: 11px;
    color: var(--fg-2);
  }
  .summary {
    font-weight: 600;
  }
  .summary.bad,
  .error {
    color: var(--error);
  }
  ol {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    gap: 8px;
    padding: 4px 0;
    border-top: 1px solid var(--rail-line);
  }
  .n {
    flex-shrink: 0;
    width: 18px;
    color: var(--fg-2);
  }
  .body {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .meta {
    font-size: 11px;
    color: var(--fg-2);
  }
  button {
    align-self: center;
    padding: 4px 12px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  button:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
</style>
