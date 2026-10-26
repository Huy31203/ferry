<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { t } from '../lib/i18n';
  import {
    applySuggestion,
    suggest,
    type LineError,
    type Media,
    type Step,
    type StepKindView,
  } from '../lib/pipelines';

  let {
    media,
    steps,
    kinds,
    onsteps,
  }: {
    media: Media;
    steps: Step[];
    kinds: StepKindView[];
    onsteps: (steps: Step[]) => void;
  } = $props();

  let text = $state('');
  let errors = $state<LineError[]>([]);
  let area: HTMLTextAreaElement | undefined;
  let suggestions = $state<string[]>([]);
  let timer: ReturnType<typeof setTimeout> | undefined;

  // The text is printed once when the mode opens; after that it is the user's.
  onMount(() => {
    void invoke<string>('pipe_print', { media, steps }).then((printed) => (text = printed));
    return () => clearTimeout(timer);
  });

  /** The line and column of the caret. */
  function caret(): { index: number; col: number; lines: string[] } {
    const upto = text.slice(0, area?.selectionStart ?? text.length);
    const lines = text.split('\n');
    const index = upto.split('\n').length - 1;
    return { index, col: upto.length - (upto.lastIndexOf('\n') + 1), lines };
  }

  function refresh() {
    const { index, col, lines } = caret();
    suggestions = suggest(lines[index] ?? '', col, kinds);
  }

  function typedSomething() {
    refresh();
    clearTimeout(timer);
    timer = setTimeout(parse, 300);
  }

  async function parse() {
    try {
      onsteps(await invoke<Step[]>('pipe_parse', { media, text }));
      errors = [];
    } catch (e) {
      errors = Array.isArray(e) ? (e as LineError[]) : [{ line: 1, message: String(e) }];
    }
  }

  function insert(suggestion: string) {
    const { index, col, lines } = caret();
    const done = applySuggestion(lines[index] ?? '', col, suggestion);
    lines[index] = done.line;
    text = lines.join('\n');
    const position = lines.slice(0, index).join('\n').length + (index > 0 ? 1 : 0) + done.col;
    queueMicrotask(() => {
      area?.focus();
      area?.setSelectionRange(position, position);
      typedSomething();
    });
  }

  function key(event: KeyboardEvent) {
    if (event.key === 'Tab' && suggestions.length > 0) {
      event.preventDefault();
      insert(suggestions[0]);
    }
  }
</script>

<div class="text-mode">
  <p class="help">{t('pipe.text.help')}</p>
  <textarea
    bind:this={area}
    bind:value={text}
    rows="10"
    spellcheck="false"
    aria-invalid={errors.length > 0}
    oninput={typedSomething}
    onkeydown={key}
    onkeyup={(event) => event.key !== 'Tab' && refresh()}
    onclick={refresh}
  ></textarea>
  {#if suggestions.length > 0}
    <div class="suggestions" aria-label={t('pipe.text.suggestions')}>
      {#each suggestions as suggestion (suggestion)}
        <button type="button" onclick={() => insert(suggestion)}>{suggestion}</button>
      {/each}
    </div>
  {/if}
  {#if errors.length > 0}
    <ul class="errors" role="alert">
      {#each errors as error (error.line + error.message)}
        <li>{t('pipe.error.syntax', { line: error.line, why: error.message })}</li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .text-mode {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .help {
    margin: 0;
    color: var(--fg-2);
  }
  textarea {
    box-sizing: border-box;
    width: 100%;
    padding: 8px;
    font-family: ui-monospace, 'SF Mono', Consolas, monospace;
    font-size: 12px;
    color: var(--fg);
    background: var(--bg-group);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    resize: vertical;
  }
  textarea[aria-invalid='true'] {
    border-color: var(--error);
  }
  .suggestions {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .suggestions button {
    padding: 2px 8px;
    font-family: ui-monospace, 'SF Mono', Consolas, monospace;
    font-size: 11px;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 999px;
    cursor: pointer;
  }
  .errors {
    margin: 0;
    padding-left: 18px;
    color: var(--error);
  }
  textarea:focus-visible,
  button:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
</style>
