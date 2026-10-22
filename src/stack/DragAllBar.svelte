<script lang="ts">
  import { startDrag } from '@crabnebula/tauri-plugin-drag';
  import { t } from '../lib/i18n';

  let { files, onclear }: { files: string[]; onclear: () => void } = $props();

  // A plain accent square stands in for the stack in the drag preview.
  function icon(): string {
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 48;
    const ctx = canvas.getContext('2d');
    if (ctx) {
      ctx.fillStyle = '#0a64d8';
      ctx.fillRect(0, 0, 48, 48);
    }
    return canvas.toDataURL('image/png');
  }

  function dragAll(event: MouseEvent) {
    if (event.button !== 0 || files.length === 0) return;
    startDrag({ item: files, icon: icon() }).catch((e) => console.warn('drag all', e));
  }
</script>

<div class="bar">
  <button type="button" class="drag" disabled={files.length === 0} onmousedown={dragAll}>
    <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 9l-3 3 3 3M9 5l3-3 3 3M15 19l-3 3-3-3M19 9l3 3-3 3M2 12h20M12 2v20" /></svg>
    {t('stack.drag_all')}
  </button>
  <button type="button" onclick={onclear}>{t('stack.clear_all')}</button>
</div>

<style>
  .bar {
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
  button:hover:not(:disabled) {
    background: var(--rail-line);
  }
  button.drag {
    cursor: grab;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
