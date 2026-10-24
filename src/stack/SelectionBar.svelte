<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { startDrag } from '@crabnebula/tauri-plugin-drag';
  import { isBusy } from '../lib/cards';
  import { fanIcon } from '../lib/dragicon';
  import { t } from '../lib/i18n';
  import type { JobUpdate } from '../lib/ipc';
  import type { Targets } from '../lib/selection';

  let {
    cards,
    ids,
    targets,
    hotAction = null,
    onall,
    onclear,
    oncrop,
    onremove,
    oncopied,
    ondrag,
  }: {
    cards: JobUpdate[];
    ids: string[];
    targets: Targets;
    hotAction?: string | null;
    onall: () => void;
    onclear: () => void;
    oncrop: () => void;
    onremove: (id: string) => void;
    /** Copy hands the keyboard back, so the next paste lands in the app the user was in. */
    oncopied: () => void;
    ondrag: (active: boolean) => void;
  } = $props();

  let note = $state<string | null>(null);
  let noteTimer: ReturnType<typeof setTimeout> | undefined;
  let icon = $state('');

  const canEdit = $derived(targets.edit.length > 0);
  const editTip = (label: string) =>
    targets.cantEdit > 0 && canEdit ? `${label} (${t('drop.list.cant_edit', { n: targets.cantEdit })})` : label;

  $effect(() => {
    const paths = cards
      .filter((c) => targets.fileIds.includes(c.id))
      .slice(-3)
      .map((c) => c.thumb_path)
      .filter((p): p is string => p !== null);
    let current = true;
    fanIcon(paths).then((url) => {
      if (current) icon = url;
    });
    return () => {
      current = false;
    };
  });

  function flash(text: string) {
    note = text;
    clearTimeout(noteTimer);
    noteTimer = setTimeout(() => (note = null), 1400);
  }

  function drag(event: MouseEvent) {
    if (event.button !== 0 || targets.files.length === 0) return;
    ondrag(true);
    startDrag({ item: targets.files, icon })
      .catch((e) => console.warn('drag selection', e))
      .finally(() => ondrag(false));
  }

  async function save() {
    try {
      const report = await invoke<{ copied: number; failed: number } | null>('list_save_to', { ids: targets.fileIds });
      if (report) flash(t('drop.list.saved'));
    } catch (e) {
      console.warn('list_save_to', e);
    }
  }

  function menu(kind: 'downscale' | 'compression' | 'convert', event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    invoke('list_menu', { ids: targets.edit, kind, x: rect.left, y: rect.bottom }).catch((e) =>
      console.warn('list_menu', e),
    );
  }

  async function copy() {
    try {
      await invoke('list_copy', { ids: targets.fileIds });
      flash(t('drop.list.copied'));
      oncopied();
    } catch (e) {
      console.warn('list_copy', e);
    }
  }

  function remove() {
    for (const card of cards.filter((c) => ids.includes(c.id))) {
      if (isBusy(card.state)) invoke('card_cancel', { id: card.id });
      onremove(card.id);
    }
  }
</script>

<div class="bar" data-bar role="toolbar" aria-label={t('drop.list.selected', { n: ids.length })}>
  <span class="count" role="status">{note ?? t('drop.list.selected', { n: ids.length })}</span>
  <button type="button" data-action="sel-drag" class:hot={hotAction === 'sel-drag'} aria-label={t('drop.list.drag')} title={t('drop.list.drag')} disabled={targets.files.length === 0} onmousedown={drag}>⇄</button>
  <button type="button" data-action="sel-save" class:hot={hotAction === 'sel-save'} aria-label={t('drop.list.save_to')} title={t('drop.list.save_to')} disabled={targets.files.length === 0} onclick={save}>⤓</button>
  <button type="button" data-action="sel-crop" class:hot={hotAction === 'sel-crop'} aria-label={t('drop.list.crop')} title={editTip(t('drop.list.crop'))} disabled={!canEdit} onclick={oncrop}>▢</button>
  <button type="button" data-action="sel-downscale" class:hot={hotAction === 'sel-downscale'} aria-label={t('drop.list.downscale')} title={editTip(t('drop.list.downscale'))} disabled={!canEdit} onclick={(e) => menu('downscale', e)}>{t('drop.list.downscale')} ▸</button>
  <button type="button" data-action="sel-compression" class:hot={hotAction === 'sel-compression'} aria-label={t('drop.list.compression')} title={editTip(t('drop.list.compression'))} disabled={!canEdit} onclick={(e) => menu('compression', e)}>{t('drop.list.compression')} ▸</button>
  <button type="button" data-action="sel-convert" class:hot={hotAction === 'sel-convert'} aria-label={t('drop.list.convert')} title={editTip(t('drop.list.convert'))} disabled={!canEdit} onclick={(e) => menu('convert', e)}>{t('drop.list.convert')} ▸</button>
  <button type="button" data-action="sel-copy" class:hot={hotAction === 'sel-copy'} aria-label={t('drop.list.copy')} title={t('drop.list.copy')} disabled={targets.files.length === 0} onclick={copy}>⧉</button>
  <button type="button" data-action="sel-remove" class:hot={hotAction === 'sel-remove'} aria-label={t('drop.list.remove')} title={t('drop.list.remove')} onclick={remove}>✕</button>
  <button type="button" data-action="sel-all" class:hot={hotAction === 'sel-all'} onclick={onall}>{t('drop.list.select_all')}</button>
  <button type="button" data-action="sel-clear" class:hot={hotAction === 'sel-clear'} onclick={onclear}>{t('drop.list.clear')}</button>
</div>

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    padding: 6px;
    border-radius: 10px;
    background: var(--rail-bg);
    color: var(--rail-fg);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2), 0 0 0 0.5px var(--rail-line);
  }
  .count {
    flex-basis: 100%;
    font-size: 12px;
    font-weight: 600;
    padding: 0 4px;
  }
  button {
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 12px;
    padding: 3px 7px;
    cursor: pointer;
  }
  button:hover:not(:disabled),
  button.hot:not(:disabled) {
    background: var(--rail-line);
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
