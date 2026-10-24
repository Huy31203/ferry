/** Mirrors ferry_core::job (Rust serialises with serde; sizes are bytes). */
export type JobState =
  | { kind: 'queued' }
  | { kind: 'optimising'; progress: number; step: string }
  | { kind: 'done' }
  | { kind: 'already_optimal' }
  | { kind: 'larger' }
  | { kind: 'error'; reason: string }
  | { kind: 'restored' };

// stream A: video (the last six)
export type Format = 'png' | 'jpeg' | 'gif' | 'webp' | 'mp4' | 'hevc' | 'av1' | 'webm' | 'mov' | 'mkv';

/** Mirrors ferry_core::edit::Size (serde tag `kind`). */
export type Size =
  | { kind: 'exact'; width: number; height: number }
  | { kind: 'width'; width: number }
  | { kind: 'height'; height: number }
  | { kind: 'long_edge'; edge: number };

/** Mirrors ferry_core::edit::Edits: the recipe every run applies to the original. */
export interface Edits {
  aspect: [number, number] | null;
  framing: 'centre' | 'smart';
  size: Size | null;
  scale: number | null;
  format: Format | null;
  compression: number | null;
  fit_under: number | null;
  level: 'normal' | 'aggressive';
}

/** What `card_edit` sends (ferry_core::edit::EditsPatch): an absent field is left alone, `null` clears it. */
export type EditsPatch = Partial<Edits>;

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
  dims_before: [number, number] | null;
  dims_after: [number, number] | null;
  format: Format | null;
  converted: boolean;
  can_edit: boolean;
  edits: Edits;
  note: string | null;
  source_format: Format | null;
  edit_hint: string | null;
}

/** Rust → stack: a menu item or hover key that ends in the page (`menu.rs`, `hover_keys.rs`). */
export interface CardAction {
  id: string;
  action: 'rename' | 'crop' | 'fit' | 'compare' | 'preview' | 'close';
}

export type Corner = 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right';

/** The settings the stack page needs (commands::stack_ready). */
export interface StackConfig {
  corner: Corner;
  card_size: 'default' | 'compact';
  auto_hide_secs: number;
  /** Where the Compression slider starts on a card without its own quality. */
  compression: number;
  crop_sizes: [number, number][];
  hover_hotkeys: boolean;
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
