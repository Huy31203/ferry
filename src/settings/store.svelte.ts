import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { withPath, type Settings } from '../lib/settings';

/** The settings the page shows. Null until `load` has answered. */
export const store = $state<{ value: Settings | null; error: string | null }>({
  value: null,
  error: null,
});

/** Saves in flight; while any are, answers and events must not overwrite what the user just changed. */
let pending = 0;
let queue: Promise<void> = Promise.resolve();

export async function load(): Promise<void> {
  store.value = await invoke<Settings>('settings_get');
  // The tray's own toggle and other windows change settings too.
  await listen<Settings>('settings:changed', (event) => {
    if (pending === 0) store.value = event.payload;
  });
}

/** Show a change at once, then save it. Rust answers with what it saved (values are clamped). */
export function change(path: string, value: unknown): void {
  if (!store.value) return;
  const next = withPath($state.snapshot(store.value) as Settings, path, value);
  store.value = next;
  store.error = null;
  pending += 1;
  queue = queue
    .then(() => invoke<Settings>('settings_set', { settings: next }))
    .then((saved) => {
      if (pending === 1) store.value = saved;
    })
    .catch(async (e) => {
      store.error = String(e);
      store.value = await invoke<Settings>('settings_get');
    })
    .finally(() => {
      pending -= 1;
    });
}
