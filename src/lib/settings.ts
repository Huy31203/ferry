export type OutputLocation = 'in_place' | 'same_folder' | 'folder' | 'temporary';

/** Mirrors ferry_core::settings::OutputRule. `folder` keeps a leading `~`. */
export interface OutputRule {
  location: OutputLocation;
  folder: string;
  template: string;
}

/** Mirrors ferry_core::settings::Settings (serde snake_case). Unknown fields from a newer file ride along untouched. */
export interface Settings {
  version: number;
  general: {
    launch_at_login: boolean;
    start_state: 'running' | 'paused' | 'stopped';
    show_dock_icon: boolean;
    show_in_screenshots: boolean;
    tray_style: 'default' | 'classic' | 'flat';
    language: string;
  };
  svc: {
    updates: { auto_check: boolean; pill: boolean; skipped_version: string };
    sync: { enabled: boolean; folder: string };
    rail: { tools: string[] };
    shortcuts: { preview: string; compare: string; step_down: string; disabled: string[] };
  };
  clipboard: {
    enabled: boolean;
    skip_multiple_files: boolean;
    max_size_mb: number;
    ignored_apps: string[];
    accumulate: boolean;
    copy_paths: boolean;
    file_template: string;
  };
  images: {
    compression: number;
    adaptive: boolean;
    switch_png_jpeg: boolean;
    strip_metadata: boolean;
    preserve_dates: boolean;
    ignored_extensions: string[];
    max_size_mb: number;
    output: { optimised: OutputRule; converted: OutputRule };
    crop_sizes: [number, number][];
  };
  floating: {
    enabled: boolean;
    corner: 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right';
    card_size: 'default' | 'compact';
    auto_hide_secs: number;
    follow_cursor: boolean;
    hover_hotkeys: boolean;
  };
  backups: { retention_days: number };
  hotkeys: { optimise: string; aggressive: string; cycle_state: string };
  clip: { auto_copy: boolean };
  video: VideoSettings;
  // Stream D
  drop_zone: {
    enabled: boolean;
    show_on: 'any_drag' | 'modifier';
    position: 'near_results' | 'under_cursor';
    modifier: 'alt' | 'shift';
    keep_original_modifier: 'alt' | 'shift';
    batch_threshold: number;
    ignored_apps: string[];
  };
  results_list: { after_cards: number };
}

// stream A: video
export interface VideoSettings {
  encoder: 'fast' | 'small' | 'visually_lossless';
  compression: number;
  convert_mov: boolean;
  audio_to_aac: boolean;
  remove_audio: boolean;
  fps_cap: 'off' | '24' | '30' | '60';
  speed_frames: 'keep' | 'drop';
  preserve_dates: boolean;
  ignored_extensions: string[];
  max_size_mb: number;
  output: { optimised: OutputRule; converted: OutputRule };
}

/** The value at a dotted path such as `images.compression`, or undefined. */
export function getPath(settings: Settings, path: string): unknown {
  let node: unknown = settings;
  for (const key of path.split('.')) {
    if (typeof node !== 'object' || node === null) return undefined;
    node = (node as Record<string, unknown>)[key];
  }
  return node;
}

/** A copy of `settings` with the value at `path` replaced; everything else, unknown fields included, is kept. */
export function withPath(settings: Settings, path: string, value: unknown): Settings {
  const next = structuredClone(settings) as unknown as Record<string, unknown>;
  const keys = path.split('.');
  const last = keys.pop() as string;
  let node = next;
  for (const key of keys) node = node[key] as Record<string, unknown>;
  node[last] = value;
  return next as unknown as Settings;
}
