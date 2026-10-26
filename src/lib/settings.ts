import type { Pipelines } from './pipelines'; // stream F

export type OutputLocation = 'in_place' | 'same_folder' | 'folder' | 'temporary';

/** Mirrors ferry_core::settings::OutputRule. `folder` keeps a leading `~`. */
export interface OutputRule {
  location: OutputLocation;
  folder: string;
  template: string;
}

/** Mirrors ferry_core::settings::Settings (serde snake_case). Unknown fields from a newer file ride along untouched. */
// Stream H: integrations
export interface AppRef {
  id: string;
  name: string;
}

// sE pdf-formats
export type ConvertTo = 'keep' | 'jpeg' | 'png' | 'webp';

/** A screen a PDF can be cropped to; mirrors ferry_core::settings::Device. */
export interface Device {
  name: string;
  width: number;
  height: number;
}

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
    optimise_files: boolean;
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
  // sE pdf-formats
  pdf: {
    dpi: number;
    dpi_aggressive: number;
    extract_dpi: number;
    max_size_mb: number;
    output: { optimised: OutputRule; converted: OutputRule };
    devices: Device[];
  };
  formats: { heic_to: ConvertTo; tiff_to: ConvertTo; avif_to: ConvertTo };
  /** Stream C: mirrors ferry_core::settings::Watch. */
  watch: Watch;
  // Stream H: integrations
  share: {
    edit_with: { image: AppRef | null; video: AppRef | null; audio: AppRef | null; pdf: AppRef | null };
    uploader: AppRef | null;
    shelf_apps: AppRef[];
  };
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
  audio: AudioSettings; // stream B: audio
  /** Stream F: mirrors ferry_core::pipeline_def::Pipelines. */
  pipelines: Pipelines;
}

/** Stream C: mirrors ferry_core::settings::WatchTypes. */
export interface WatchTypes {
  image: boolean;
  video: boolean;
  audio: boolean;
  pdf: boolean;
}

/** Stream C: mirrors ferry_core::settings::WatchedFolder. */
export interface WatchedFolder {
  path: string;
  types: WatchTypes;
  show_results: boolean;
  ignore: string[];
  max_files_at_once: number;
  paused: boolean;
  /** Stream F: the library pipeline its files run through; null is the plain optimise. */
  pipeline: number | null;
}

/** Stream C: mirrors ferry_core::settings::Watch. */
export interface Watch {
  captures: { screenshots: boolean; recordings: boolean; show_results: boolean };
  folders: WatchedFolder[];
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

// ---- stream B: audio ----
type AudioFormat = 'mp3' | 'm4a' | 'opus' | 'wav' | 'flac';

export interface AudioSettings {
  outputs: { wav: AudioFormat; aiff: AudioFormat; flac: AudioFormat; mp3: AudioFormat; m4a: AudioFormat; ogg: AudioFormat; opus: AudioFormat };
  bitrate_kbps: number;
  opus_bitrate_kbps: number;
  loudness: 'off' | 'streaming' | 'apple_music' | 'podcast' | 'broadcast';
  cover: 'keep' | 'optimise' | 'downscale' | 'remove';
  cover_size: { shape: 'square' | 'long_edge'; side: number };
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
