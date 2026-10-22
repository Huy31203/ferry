/** Mirrors ferry_core::job (Rust serialises with serde; sizes are bytes). */
export type JobState =
  | { kind: 'queued' }
  | { kind: 'optimising'; progress: number; step: string }
  | { kind: 'done' }
  | { kind: 'already_optimal' }
  | { kind: 'larger' }
  | { kind: 'error'; reason: string }
  | { kind: 'restored' };

export interface JobUpdate {
  id: string;
  state: JobState;
  name: string;
  ext: string;
  before: number;
  after: number | null;
  detail: string;
  thumb_path: string | null;
  source: 'clipboard' | 'file';
  can_restore: boolean;
  can_retry: boolean;
  output: string | null;
}

export type Corner = 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right';

/** The floating-results settings the stack page needs (commands::stack_ready). */
export interface StackConfig {
  corner: Corner;
  card_size: 'default' | 'compact';
  auto_hide_secs: number;
}

export const isMac = navigator.userAgent.includes('Mac');

/** Tag <html> with os-mac / os-windows so theme.css can switch fonts and radii. */
export function applyOsClass(): void {
  document.documentElement.classList.add(isMac ? 'os-mac' : 'os-windows');
}

/** On macOS, Rust answers with the user's accent colour, already adjusted for white text on top; Windows keeps the default (spec §7). */
export async function applyAccent(): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  const hex = await invoke<string | null>('accent_color');
  if (!hex) return;
  document.documentElement.style.setProperty('--accent', hex);
  document.documentElement.dataset.accent = 'custom';
}
