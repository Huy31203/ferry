<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { t } from '../lib/i18n';
  import { checkForm, LEVELS, VOLUMES } from './form';

  type View = { folder: string; name: string; count: number; cores: number; level: number; threads: number };

  let cores = $state(1);
  let folder = $state('');
  let name = $state('');
  let level = $state(5);
  let threads = $state(1);
  let split = $state('');
  let password = $state('');
  let again = $state('');
  let show = $state(false);
  let encryption = $state<'aes256' | 'zipcrypto'>('aes256');
  let eta = $state('');
  let error = $state('');

  const problem = $derived(checkForm({ name, password, again, split }));

  async function load() {
    const v = await invoke<View>('archive_view');
    ({ folder, name, level, threads, cores } = v);
    password = again = error = '';
  }

  onMount(() => {
    void load();
    const off = listen('archive:open', () => void load());
    return () => void off.then((fn) => fn());
  });

  $effect(() => {
    void invoke<string>('archive_estimate', { level, threads }).then((s) => (eta = s));
  });

  async function pick() {
    const got = await invoke<[string, string] | null>('archive_pick', { folder, name });
    if (got) [folder, name] = got;
  }

  async function ok() {
    if (problem) return;
    try {
      await invoke('archive_start', {
        req: { folder, name, level, threads, password: password || null, encryption, split: split || null },
      });
    } catch (e) {
      error = String(e);
    }
  }

  const close = () => void invoke('archive_close');

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
    else if (e.key === 'Enter') void ok();
  }
</script>

<svelte:window {onkeydown} />

<main>
  <div class="cols">
    <section>
      <label for="name">{t('archive.archive')}</label>
      <small class="folder">{folder}</small>
      <div class="row">
        <input id="name" type="text" bind:value={name} />
        <button type="button" onclick={pick}>{t('archive.choose')}</button>
      </div>
      <label for="level">{t('archive.level')}</label>
      <select id="level" bind:value={level}>
        {#each LEVELS as l (l)}
          <option value={l}>{t(`archive.level.${l}`)}</option>
        {/each}
      </select>
      <label for="threads">{t('archive.threads')}</label>
      <div class="row">
        <select id="threads" bind:value={threads}>
          {#each Array.from({ length: cores }, (_, i) => i + 1) as n (n)}
            <option value={n}>{n}</option>
          {/each}
        </select>
        <span>/ {cores}</span>
      </div>
      <label for="split">{t('archive.split')}</label>
      <input id="split" list="volumes" bind:value={split} />
      <datalist id="volumes">
        {#each VOLUMES as v (v)}<option value={v}></option>{/each}
      </datalist>
      <small>{t('archive.split.help')}</small>
      <p>{t('archive.estimate', { eta })}</p>
    </section>
    <section class="group">
      <h2>{t('archive.encryption')}</h2>
      <label for="pw">{t('archive.password')}</label>
      <input id="pw" type={show ? 'text' : 'password'} bind:value={password} />
      <label for="pw2">{t('archive.password_again')}</label>
      <input id="pw2" type={show ? 'text' : 'password'} bind:value={again} />
      <label class="check"><input type="checkbox" bind:checked={show} />{t('archive.show_password')}</label>
      <label for="method">{t('archive.method')}</label>
      <select id="method" bind:value={encryption}>
        <option value="aes256">{t('archive.method.aes256')}</option>
        <option value="zipcrypto">{t('archive.method.zipcrypto')}</option>
      </select>
      <small>{encryption === 'aes256' ? t('archive.method.aes.help') : t('archive.method.zipcrypto.help')}</small>
    </section>
  </div>
  <footer>
    <span class="error">{problem ? t(problem) : error}</span>
    <button type="button" onclick={close}>{t('archive.cancel')}</button>
    <button type="button" class="primary" disabled={!!problem} onclick={ok}>{t('archive.ok')}</button>
  </footer>
</main>

<style>
  :global(body) { margin: 0; background: var(--bg-content); color: var(--fg); font: 13px var(--font-ui); }
  main { display: flex; flex-direction: column; height: 100vh; padding: 16px; box-sizing: border-box; gap: 12px; }
  .cols { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 16px; flex: 1; }
  section { display: flex; flex-direction: column; gap: 6px; }
  .group { background: var(--bg-group); border-radius: var(--card-radius); padding: 12px; align-self: start; }
  h2 { margin: 0 0 4px; font-size: 13px; }
  label { font-weight: 600; margin-top: 4px; }
  .check { font-weight: 400; display: flex; gap: 6px; align-items: center; }
  .row { display: flex; gap: 6px; align-items: center; }
  .row input { flex: 1; }
  small { color: var(--fg-2); }
  .folder { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  p { margin: 4px 0 0; }
  footer { display: flex; gap: 8px; align-items: center; justify-content: flex-end; }
  .error { flex: 1; color: var(--error); }
  .primary { background: var(--accent); color: #fff; border: 0; border-radius: 6px; padding: 4px 14px; }
  .primary:disabled { opacity: 0.5; }
</style>
