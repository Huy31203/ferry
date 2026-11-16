import { t } from '../lib/i18n';

/** `grip`: the shelf holds items and nothing is happening, so only its grip shows. */
export type StripState = 'hidden' | 'grip' | 'resting' | 'open' | 'target' | 'cleared';

/** Mirrors the Rust `shelf:drag` event. */
export interface DragView {
  /** A drag the shelf can take (files, from any app or from Ferry's cards). */
  offer: boolean;
  /** The pointer is within reach of the strip, and not over the drop zone. */
  near: boolean;
  /** The shelf's own pile (or a Show items row) is being dragged out. */
  own: boolean;
}

export interface StripInput {
  count: number;
  canUndo: boolean;
  hover: boolean;
  /** Opened from the tray's "Show shelf" for a few seconds. */
  summoned: boolean;
  drag: DragView;
}

export const NO_DRAG: DragView = { offer: false, near: false, own: false };
export const STRIP = 27;
export const EXPANDED = 170;
/** The panel while "Show items…" lists them. */
export const LIST = 300;
export const GRIP = 16;
export const WIDTH = 150;
export const LIST_WIDTH = 240;
export const COLLAPSE_MS = 300;
/** The panel's slide; Rust keeps the window this long after a shrink (shelf/window.rs). */
export const SLIDE_MS = 340;
export const UNDO_MS = 5000;
export const SUMMON_MS = 4000;

/** Empty and idle it is hidden; holding items it shows only its grip. */
export function stripState({ count, canUndo, hover, summoned, drag }: StripInput): StripState {
  if (drag.own) return 'open';
  if (drag.offer && drag.near) return 'target';
  if (count === 0 && canUndo) return 'cleared';
  if (hover || summoned) return 'open';
  if (drag.offer) return 'resting';
  if (count > 0) return 'grip';
  return 'hidden';
}

export function stripHeight(state: StripState, listing = false): number {
  if (state === 'hidden') return 0;
  if (state === 'grip') return GRIP;
  if (state === 'open') return listing ? LIST : EXPANDED;
  if (state === 'target') return EXPANDED;
  return STRIP;
}

export function stripWidth(state: StripState, listing = false): number {
  return state === 'open' && listing ? LIST_WIDTH : WIDTH;
}

export function countLabel(count: number): string {
  return count === 1 ? t('shelf.item') : t('shelf.items', { count });
}
