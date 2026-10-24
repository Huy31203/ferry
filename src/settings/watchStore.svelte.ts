// Stream C: what the Watched folders pane shows, shared by its two controls.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Support, WatchStatus } from '../lib/watch';

export const watch = $state<{ status: WatchStatus | null; support: Support | null }>({
  status: null,
  support: null,
});

let loading: Promise<void> | null = null;

/** Ask once, then follow `watch:status`. Calling it again does nothing. */
export function loadWatch(): Promise<void> {
  loading ??= (async () => {
    await listen<WatchStatus>('watch:status', (event) => (watch.status = event.payload));
    watch.status = await invoke<WatchStatus>('watch_status');
    watch.support = await invoke<Support>('watch_support');
  })();
  return loading;
}
