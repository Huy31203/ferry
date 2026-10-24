/** Mirrors of stream G's Rust serde shapes: hotkeys, updates and (later) sync. */

export type HotkeyId =
  | 'optimise'
  | 'aggressive'
  | 'cycle_state'
  | 'preview'
  | 'compare'
  | 'step_down'
  | 'scale';

/** ferry `hotkeys::Problem`. */
export type Problem =
  | { kind: 'invalid' }
  | { kind: 'needs_modifier' }
  | { kind: 'reserved' }
  | { kind: 'card_key' }
  | { kind: 'duplicate'; with: string };

/** ferry `hotkeys::Verdict`. */
export interface Verdict {
  problem: Problem | null;
  warning: 'dev_tools' | null;
}

/** ferry `hotkeys::Status`. */
export interface HotkeyStatus {
  id: string;
  group: 'global' | 'hover';
  accelerator: string | null;
  enabled: boolean;
  state: 'registered' | 'ready' | 'off' | 'failed' | 'blocked';
  problem: Problem | null;
}

/** ferry `svc_commands::PaneRequest`: where Settings should open. */
export interface PaneRequest {
  pane: string;
  row: string | null;
}

/** ferry `updater::UpdateState` (serde tag `kind`). */
export type UpdateState =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'up_to_date' }
  | { kind: 'available'; version: string; notes: string }
  | { kind: 'downloading'; version: string; received: number; total: number | null }
  | { kind: 'ready'; version: string; busy: number }
  | { kind: 'installing'; version: string }
  | { kind: 'failed'; phase: 'check' | 'download' | 'install'; reason: string };

/** ferry `updater::UpdateView`. */
export interface UpdateView {
  current: string;
  state: UpdateState;
  skipped: boolean;
  pill: boolean;
  release_url: string | null;
}
