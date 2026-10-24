<script lang="ts">
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { startDrag } from '@crabnebula/tauri-plugin-drag';
  import { autoHides, canDrag, isBusy, type Tool } from '../lib/cards';
  import { formatBytes, savingPercent } from '../lib/format';
  import { t, type StringKey } from '../lib/i18n';
  import type { JobUpdate } from '../lib/ipc';

  let {
    card,
    hovered = false,
    held = false,
    selecting = false,
    checked = false,
    ring = false,
    hotAction = null,
    autoHideSecs,
    tool = null,
    onclose,
    ondrag,
    ontool,
    ontoggle,
  }: {
    card: JobUpdate;
    hovered?: boolean;
    /** The list holds every row open (pointer on the list, a selection, a drag). */
    held?: boolean;
    /** Something is selected, so a click on the row toggles it instead of starting a drag. */
    selecting?: boolean;
    checked?: boolean;
    /** The keyboard cursor is on this row (only while the list has the keyboard). */
    ring?: boolean;
    hotAction?: string | null;
    autoHideSecs: number;
    tool?: Tool | null;
    onclose: () => void;
    ondrag: (active: boolean) => void;
    ontool: (kind: Tool | null) => void;
    ontoggle: (shift: boolean) => void;
  } = $props();

  let focused = $state(false);
  let thumb = $state<HTMLImageElement>();
  let renameError = $state<string | null>(null);

  const busy = $derived(isBusy(card.state));
  const fileName = $derived(card.ext ? `${card.name}.${card.ext}` : card.name);
  const canRename = $derived(canDrag(card));
  const job = $derived(card.state);
  const saving = $derived(
    job.kind === 'done' && card.after !== null ? savingPercent(card.before, card.after) : null,
  );

  // Finished rows fade out after the configured time; hover, focus or a hold keeps them (§5.5).
  $effect(() => {
    if (!autoHides(job) || autoHideSecs === 0 || hovered || focused || held || tool !== null) return;
    const timer = setTimeout(onclose, autoHideSecs * 1000);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    if (tool !== 'rename') renameError = null;
  });

  function act(command: string, args: Record<string, unknown> = {}) {
    invoke(command, { id: card.id, ...args }).catch((e) => console.warn(command, e));
  }

  function openMenu(event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    act('card_menu', { x: rect.left, y: rect.bottom + 4 });
  }

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

  // A drag preview the size of the thumbnail, not of the full image.
  function dragIcon(): string {
    const canvas = document.createElement('canvas');
    canvas.width = 64;
    canvas.height = 64;
    const ctx = canvas.getContext('2d');
    if (ctx && thumb?.complete && thumb.naturalWidth > 0) {
      const scale = Math.min(64 / thumb.naturalWidth, 64 / thumb.naturalHeight);
      const w = thumb.naturalWidth * scale;
      const h = thumb.naturalHeight * scale;
      ctx.drawImage(thumb, (64 - w) / 2, (64 - h) / 2, w, h);
    }
    return canvas.toDataURL('image/png');
  }

  function press(event: MouseEvent) {
    if (event.button !== 0 || selecting || !canDrag(card)) return;
    if ((event.target as HTMLElement).closest('button, input')) return;
    ondrag(true);
    startDrag({ item: [card.output as string], icon: dragIcon() })
      .catch((e) => console.warn('drag', e))
      .finally(() => ondrag(false));
  }

  function click(event: MouseEvent) {
    if (selecting && !(event.target as HTMLElement).closest('button, input')) ontoggle(event.shiftKey);
  }

  const stateLabel = $derived(t(`card.state.${job.kind}` as StringKey));
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="row"
  class:hover={hovered}
  class:checked
  class:ring
  id="row-{card.id}"
  role="option"
  tabindex="-1"
  aria-selected={checked}
  aria-label={t('card.aria', { name: fileName, state: stateLabel })}
  data-card-id={card.id}
  onmousedown={press}
  onclick={click}
  oncontextmenu={(event) => {
    event.preventDefault();
    act('card_menu', { x: event.clientX, y: event.clientY });
  }}
  onfocusin={() => (focused = true)}
  onfocusout={() => (focused = false)}
>
  <span class="thumb">
    {#if card.thumb_path}
      <img bind:this={thumb} src={convertFileSrc(card.thumb_path)} alt="" width="32" height="32" crossorigin="anonymous" />
    {/if}
    {#if hovered || checked || focused || selecting}
      <input type="checkbox" {checked} aria-label={t('drop.list.select', { name: fileName })} onclick={(e) => ontoggle(e.shiftKey)} />
    {/if}
  </span>
  <span class="main">
    {#if tool === 'rename'}
      <input
        class="name-field"
        type="text"
        value={card.name}
        aria-label={t('card.rename_named', { name: fileName })}
        aria-invalid={renameError !== null}
        use:focusSelect
        onkeydown={renameKey}
        onblur={() => ontool(null)}
      />
    {:else}
      <span class="name" title={fileName}>{card.name}</span>
    {/if}
    {#if busy && job.kind === 'optimising'}
      <span class="step">{job.step}</span>
    {:else}
      <span class="sizes">
        {formatBytes(card.before)}{card.after !== null ? ` → ${formatBytes(card.after)}` : ''}
      </span>
    {/if}
  </span>
  {#if card.ext}<span class="chip">{card.ext}</span>{/if}
  <span class="pill {job.kind}" title={job.kind === 'error' ? job.reason : ''}>
    <span aria-hidden="true">{job.kind === 'done' ? '✓' : job.kind === 'error' ? '!' : busy ? '…' : '–'}</span>
    {saving ?? stateLabel}
  </span>
  {#if hovered || focused}
    <span class="rail">
      <button type="button" data-action="copy" class:hot={hotAction === 'copy'} aria-label={t('card.copy')} disabled={!canDrag(card)} onclick={() => act('card_copy')}>⧉</button>
      {#if card.can_restore}
        <button type="button" data-action="restore" class:hot={hotAction === 'restore'} aria-label={t('card.restore')} onclick={() => act('card_restore')}>↺</button>
      {/if}
      <button type="button" data-action="more" class:hot={hotAction === 'more'} aria-label={t('card.menu')} onclick={openMenu}>⋯</button>
    </span>
  {/if}
  {#if busy}
    <span class="bar" style:width="{job.kind === 'optimising' ? Math.round(job.progress * 100) : 0}%"></span>
  {/if}
</div>

<style>
  .row {
    position: relative;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 44px;
    padding: 0 8px;
    color: var(--rail-fg);
    user-select: none;
  }
  .row.hover,
  .row.checked {
    background: var(--rail-line);
  }
  .row.ring {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .thumb {
    position: relative;
    width: 32px;
    height: 32px;
    flex: none;
    border-radius: 4px;
    background: var(--rail-line);
    overflow: hidden;
  }
  .thumb img {
    display: block;
    object-fit: cover;
  }
  .thumb input {
    position: absolute;
    top: 2px;
    left: 2px;
    margin: 0;
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .name {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-weight: 600;
    font-size: 12px;
  }
  .name-field {
    font: inherit;
    font-size: 12px;
    min-width: 0;
  }
  .sizes,
  .step {
    font-size: 11px;
    color: var(--fg-2);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .chip {
    flex: none;
    font-size: 10px;
    text-transform: uppercase;
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--rail-line);
  }
  .pill {
    flex: none;
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
  }
  .pill.done {
    color: #1e8e3e;
  }
  .pill.already_optimal,
  .pill.larger {
    color: #b06000;
  }
  .pill.error {
    color: var(--error);
  }
  .rail {
    display: flex;
    gap: 2px;
  }
  .rail button {
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    border-radius: 6px;
    padding: 2px 6px;
    cursor: pointer;
  }
  .rail button:hover,
  .rail button.hot {
    background: var(--rail-line);
  }
  .bar {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 2px;
    background: var(--accent);
  }
  @media (prefers-color-scheme: dark) {
    .pill.done {
      color: #81c995;
    }
    .pill.already_optimal,
    .pill.larger {
      color: #fdd663;
    }
  }
</style>
