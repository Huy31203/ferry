/** Mirrors ferry_core::{intake, batch} (Rust serialises with serde; sizes are bytes). */
import type { Format } from './ipc';

export type MediaKind = 'image' | 'video' | 'audio' | 'pdf';

export type RowStatus =
  | 'waiting'
  | 'working'
  | 'done'
  | 'already_optimal'
  | 'larger'
  | 'skipped'
  | 'failed'
  | 'cancelled'
  | 'restored';

export interface BatchRow {
  index: number;
  path: string;
  name: string;
  ext: string;
  kind: MediaKind | null;
  before: number;
  after: number | null;
  dims_before: [number, number] | null;
  dims_after: [number, number] | null;
  status: RowStatus;
  reason: string | null;
  progress: number;
  step: string | null;
  job: string | null;
  thumb: string | null;
  output: string | null;
  can_restore: boolean;
  can_edit: boolean;
  keep_original: boolean;
}

export interface Tally {
  total: number;
  waiting: number;
  working: number;
  done: number;
  already_optimal: number;
  larger: number;
  skipped: number;
  failed: number;
  cancelled: number;
  restored: number;
  before: number;
  after: number;
}

export type Phase = 'empty' | 'running' | 'paused' | 'done' | 'rolling_back';

export interface BatchStatus {
  phase: Phase;
  tally: Tally;
  rate: number;
  eta_secs: number | null;
  rolling: [number, number] | null;
  skipped_in_folders: number;
  truncated: boolean;
}

export interface Adjust {
  compression: number | null;
  scale: number | null;
  format: Format | null;
  extreme: boolean;
}

export interface Snapshot {
  rows: BatchRow[];
  status: BatchStatus;
  adjust: Adjust;
  threshold: number;
}

/** `batch:open`: a row menu item that ends in a window of its own. */
export interface BatchOpen {
  id: string;
  action: 'compare' | 'preview';
}

/** Mirrors ferry_core::intake::Summary. */
export interface Summary {
  files: number;
  folders: number;
  images: number;
  videos: number;
  audio: number;
  pdfs: number;
  unsupported: number;
  unsupported_ext: string | null;
}

export type ZoneState = 'hidden' | 'idle' | 'hover' | 'unsupported' | 'dropped';
export type ZoneMode = 'zone' | 'tab';

/** `drop:view`: what the zone page shows. */
export interface ZoneView {
  state: ZoneState;
  mode: ZoneMode;
  summary: Summary | null;
  keep_original: boolean;
  opens_batch: boolean;
}

/** `list_config`: where the stack switches to the compact list. */
export interface ListConfig {
  after_cards: number;
}
