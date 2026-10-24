<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, tick } from 'svelte';
  import { matches } from '../lib/fuzzy';
  import { t } from '../lib/i18n';
  import { isMac } from '../lib/ipc';
  import { panes, settingsFor, type PaneId, type Setting } from '../lib/schema';
  import { getPath } from '../lib/settings';
  import type { PaneRequest } from '../lib/svc';
  import Row from './Row.svelte';
  import { change, load, store } from './store.svelte';
  import { loadWatch, watch } from './watchStore.svelte'; // stream C

  const all = settingsFor(isMac);
  const paneLabel = (id: PaneId) => t(panes.find((p) => p.id === id)!.label);

  let pane = $state<PaneId>('general');
  let query = $state('');
  let highlighted = $state<string | null>(null);

  const searching = $derived(query.trim() !== '');
  const results = $derived(
    all.filter((s) =>
      matches(query, t(s.label), s.help ? t(s.help) : '', paneLabel(s.pane)),
    ),
  );
  const rows = $derived(all.filter((s) => s.pane === pane));

  /** The rows of the open pane, split into runs that share a group heading. */
  const groups = $derived.by(() => {
    const out: { heading: Setting['group']; rows: Setting[] }[] = [];
    for (const setting of rows) {
      const last = out[out.length - 1];
      if (last && last.heading === setting.group) last.rows.push(setting);
      else out.push({ heading: setting.group, rows: [setting] });
    }
    return out;
  });

  onMount(() => {
    void load().then(takePane);
    void loadWatch(); // stream C
    // svc: another part of Ferry opens Settings at a pane (and a row)
    const unlisten = listen('svc:open-pane', () => void takePane());
    return () => void unlisten.then((off) => off());
  });

  // stream C: a row that needs media this build cannot handle yet.
  function unsupported(setting: Setting): boolean {
    return !!setting.requires && watch.support !== null && !watch.support[setting.requires];
  }

  async function takePane() {
    const request = await invoke<PaneRequest | null>('svc_settings_pane');
    if (!request) return;
    const target = all.find((s) => s.id === request.row);
    if (target) await jump(target);
    else if (panes.some((p) => p.id === request.pane)) pane = request.pane as PaneId;
  }

  /** The switch a row depends on, when it is off. */
  function blocker(setting: Setting): Setting | null {
    if (!setting.depends_on || !store.value) return null;
    if (getPath(store.value, setting.depends_on)) return null;
    return all.find((s) => s.path === setting.depends_on) ?? null;
  }

  async function jump(setting: Setting) {
    pane = setting.pane;
    query = '';
    await tick();
    document.getElementById(`row-${setting.id}`)?.scrollIntoView({ block: 'center' });
    highlighted = setting.id;
    setTimeout(() => {
      if (highlighted === setting.id) highlighted = null;
    }, 2000);
  }
</script>

<div class="window">
  <nav class="sidebar" aria-label={t('settings.window_title')}>
    <input
      type="search"
      class="search"
      placeholder={t('settings.search')}
      aria-label={t('settings.search')}
      bind:value={query}
    />
    {#if searching}
      {#if results.length === 0}
        <p class="empty">{t('settings.no_match')}</p>
      {:else}
        <ul class="results">
          {#each results as result (result.id)}
            <li>
              <button type="button" onclick={() => jump(result)}>
                <span>{t(result.label)}</span>
                <small>{paneLabel(result.pane)}</small>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {:else}
      <ul class="panes">
        {#each panes as p (p.id)}
          <li>
            <button
              type="button"
              class:selected={pane === p.id}
              aria-current={pane === p.id ? 'page' : undefined}
              onclick={() => (pane = p.id)}>{t(p.label)}</button
            >
          </li>
        {/each}
      </ul>
    {/if}
  </nav>

  <main class="content">
    <h1>{paneLabel(pane)}</h1>
    {#if store.error}
      <p class="error" role="alert">{t('settings.failed', { why: store.error })}</p>
    {/if}
    {#if store.value}
      {#each groups as g (g.heading ?? 'main')}
        {#if g.heading}<h2>{t(g.heading)}</h2>{/if}
        <div class="group">
          {#each g.rows as setting (setting.id)}
            {@const lock = blocker(setting)}
            <Row
              {setting}
              value={setting.path ? getPath(store.value, setting.path) : undefined}
              disabled={lock !== null || unsupported(setting)}
              hint={lock
                ? t('settings.turn_on_first', { name: t(lock.label) })
                : unsupported(setting)
                  ? t('watch.unsupported')
                  : null}
              highlight={highlighted === setting.id}
              onchange={(value) => setting.path && change(setting.path, value)}
            />
          {/each}
        </div>
      {/each}
    {/if}
  </main>
</div>

<style>
  .window {
    display: flex;
    height: 100vh;
  }
  .sidebar {
    flex: 0 0 210px;
    padding: 14px 10px;
    overflow-y: auto;
    background: var(--bg-sidebar);
  }
  .search {
    box-sizing: border-box;
    width: 100%;
    margin-bottom: 10px;
    padding: 5px 8px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .sidebar button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    width: 100%;
    padding: 6px 10px;
    font: inherit;
    color: var(--fg);
    text-align: left;
    background: none;
    border: 0;
    border-radius: 6px;
    cursor: pointer;
  }
  .sidebar button.selected {
    background: var(--accent);
    color: #fff;
  }
  .sidebar button:focus-visible,
  .search:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  small {
    color: var(--fg-2);
    font-size: 11px;
  }
  .empty {
    padding: 0 10px;
    color: var(--fg-2);
  }
  .content {
    flex: 1;
    padding: 20px 24px;
    overflow-y: auto;
  }
  h1 {
    margin: 0 0 14px;
    font-size: 20px;
    font-weight: 600;
  }
  h2 {
    margin: 22px 0 8px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--fg-2);
  }
  .group {
    background: var(--bg-group);
    border-radius: 10px;
  }
  .error {
    color: var(--error);
  }
</style>
