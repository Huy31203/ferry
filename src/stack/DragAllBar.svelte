<script lang="ts">
  import { startDrag } from '@crabnebula/tauri-plugin-drag';
  import { t } from '../lib/i18n';
  import { fanIcon } from '../lib/dragicon';
  import type { JobUpdate } from '../lib/ipc';

  let {
    cards,
    hotAction = null,
    above = false,
    onclear,
    ondrag,
  }: {
    /** The finished cards whose files "Drag all" carries. */
    cards: JobUpdate[];
    hotAction?: string | null;
    /** Put tooltips above the bar, for a bar at the bottom of the stack. */
    above?: boolean;
    onclear: () => void;
    ondrag: (active: boolean) => void;
  } = $props();

  const files = $derived(cards.map((card) => card.output as string));
  const tips: Record<string, string> = {
    drag: t('stack.drag_all.tip'),
    clear: t('stack.clear_all.tip'),
  };

  // The preview is a small fan of the newest thumbnails, built ahead of time so the drag starts at once.
  let icon = $state('');
  $effect(() => {
    const paths = cards
      .slice(-3)
      .map((card) => card.thumb_path)
      .filter((path): path is string => path !== null);
    let current = true;
    fanIcon(paths).then((url) => {
      if (current) icon = url;
    });
    return () => {
      current = false;
    };
  });

  function dragAll(event: MouseEvent) {
    if (event.button !== 0 || files.length === 0) return;
    ondrag(true);
    startDrag({ item: files, icon })
      .catch((e) => console.warn('drag all', e))
      .finally(() => ondrag(false));
  }
</script>

<div class="bar" data-bar>
  <button
    type="button"
    class="drag"
    class:hot={hotAction === 'drag'}
    data-action="drag"
    disabled={files.length === 0}
    onmousedown={dragAll}
  >
    <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 9l-3 3 3 3M9 5l3-3 3 3M15 19l-3 3-3-3M19 9l3 3-3 3M2 12h20M12 2v20" /></svg>
    {t('stack.drag_all')}
  </button>
  <button type="button" class:hot={hotAction === 'clear'} data-action="clear" onclick={onclear}>
    {t('stack.clear_all')}
  </button>
  {#if hotAction === 'drag' || hotAction === 'clear'}
    <div class="tip" role="tooltip" class:above class:end={hotAction === 'clear'}>{tips[hotAction]}</div>
  {/if}
</div>

<style>
  .bar {
    position: relative;
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 6px;
    border-radius: 10px;
    background: var(--rail-bg);
    color: var(--rail-fg);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2), 0 0 0 0.5px var(--rail-line);
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    padding: 4px 10px;
    cursor: pointer;
  }
  button:hover:not(:disabled),
  button.hot:not(:disabled) {
    background: var(--rail-line);
  }
  button:active:not(:disabled) {
    transform: scale(0.96);
  }
  button.drag {
    cursor: grab;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .tip {
    position: absolute;
    z-index: 5;
    left: 6px;
    top: calc(100% + 6px);
    padding: 4px 8px;
    border-radius: 6px;
    background: rgba(20, 20, 22, 0.94);
    color: #fff;
    font-size: 11px;
    font-weight: 500;
    white-space: nowrap;
    pointer-events: none;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4), 0 0 0 0.5px rgba(255, 255, 255, 0.15);
  }
  .tip.above {
    top: auto;
    bottom: calc(100% + 6px);
  }
  .tip.end {
    left: auto;
    right: 6px;
  }
</style>
