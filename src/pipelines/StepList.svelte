<script lang="ts">
  import { t, type StringKey } from '../lib/i18n';
  import type { Issue, Step, StepKindView } from '../lib/pipelines';

  let {
    steps,
    kinds,
    captions,
    issues,
    selected,
    onselect,
    onmove,
    onremove,
    onadd,
  }: {
    steps: Step[];
    kinds: StepKindView[];
    captions: string[];
    issues: Issue[];
    selected: number | null;
    onselect: (i: number) => void;
    onmove: (from: number, to: number) => void;
    onremove: (i: number) => void;
    onadd: (kind: StepKindView) => void;
  } = $props();

  let rows: HTMLElement[] = $state([]);
  /** The step being dragged and the place it would land. */
  let drag = $state<{ from: number; over: number } | null>(null);

  const known = (step: Step) => kinds.some((k) => k.id === step.kind);
  const issueOf = (i: number) => issues.find((issue) => issue.step === i)?.message ?? null;

  // Pointer events rather than HTML5 drag and drop, which Tauri's file-drop handler intercepts on Windows.
  function start(event: PointerEvent, from: number) {
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    drag = { from, over: from };
  }

  function over(event: PointerEvent) {
    if (!drag) return;
    const found = rows.findIndex((row) => {
      const box = row.getBoundingClientRect();
      return event.clientY >= box.top && event.clientY <= box.bottom;
    });
    if (found !== -1) drag = { ...drag, over: found };
  }

  function end() {
    if (drag && drag.over !== drag.from) onmove(drag.from, drag.over);
    drag = null;
  }

  function add(event: Event & { currentTarget: HTMLSelectElement }) {
    const kind = kinds.find((k) => k.id === event.currentTarget.value);
    event.currentTarget.value = '';
    if (kind) onadd(kind);
  }
</script>

<div class="steps">
  <ol>
    {#each steps as step, i (i)}
      {@const issue = issueOf(i)}
      <li
        bind:this={rows[i]}
        class:selected={selected === i}
        class:target={drag !== null && drag.over === i && drag.from !== i}
      >
        <button
          type="button"
          class="handle"
          aria-label={t('pipe.drag_handle')}
          onpointerdown={(event) => start(event, i)}
          onpointermove={over}
          onpointerup={end}
          onpointercancel={() => (drag = null)}>⠿</button
        >
        <button type="button" class="card" aria-current={selected === i ? 'true' : undefined} onclick={() => onselect(i)}>
          <span class="number">{i + 1}</span>
          <span class="text">
            <span class="caption">{captions[i] ?? step.kind}</span>
            {#if issue}<span class="issue" role="alert">{issue}</span>{/if}
          </span>
        </button>
        {#if known(step)}
          <button type="button" class="small" aria-label={t('pipe.move_up')} disabled={i === 0} onclick={() => onmove(i, i - 1)}>↑</button>
          <button type="button" class="small" aria-label={t('pipe.move_down')} disabled={i === steps.length - 1} onclick={() => onmove(i, i + 1)}>↓</button>
        {/if}
        <button type="button" class="small" aria-label={t('pipe.remove_step')} onclick={() => onremove(i)}>×</button>
      </li>
    {/each}
  </ol>
  <label class="add">
    <span class="sr">{t('pipe.add_step')}</span>
    <select onchange={add}>
      <option value="">{t('pipe.add_step')}</option>
      {#each kinds as kind (kind.id)}
        <option value={kind.id}>{t(kind.label as StringKey)}</option>
      {/each}
    </select>
  </label>
</div>

<style>
  .steps {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  ol {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: stretch;
    gap: 4px;
    margin-bottom: 6px;
    border-radius: 8px;
  }
  li.target {
    outline: 2px dashed var(--accent);
  }
  .card {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 10px;
    min-width: 0;
    padding: 8px 10px;
    font: inherit;
    text-align: left;
    color: var(--fg);
    background: var(--bg-group);
    border: 1px solid var(--rail-line);
    border-radius: 8px;
    cursor: pointer;
  }
  li.selected .card {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .number {
    flex-shrink: 0;
    width: 22px;
    height: 22px;
    line-height: 22px;
    text-align: center;
    color: var(--fg-2);
    border: 1px solid var(--rail-line);
    border-radius: 50%;
    font-size: 11px;
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .caption {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .issue {
    font-size: 11px;
    color: var(--error);
  }
  .handle,
  .small {
    padding: 0 6px;
    font: inherit;
    color: var(--fg-2);
    background: none;
    border: 0;
    cursor: pointer;
  }
  .handle {
    cursor: grab;
    touch-action: none;
  }
  .small:disabled {
    opacity: 0.35;
    cursor: default;
  }
  select {
    padding: 4px 8px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  button:focus-visible,
  select:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
</style>
