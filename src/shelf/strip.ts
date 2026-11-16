import { t } from '../lib/i18n';

export type StripState = 'hidden' | 'resting' | 'open' | 'target' | 'cleared';

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
  drag: DragView;
}

export const NO_DRAG: DragView = { offer: false, near: false, own: false };
export const STRIP = 27;
export const EXPANDED = 170;
export const COLLAPSE_MS = 300;
export const UNDO_MS = 5000;

export function stripState({ count, canUndo, hover, drag }: StripInput): StripState {
  if (drag.own) return 'open';
  if (drag.offer && drag.near) return 'target';
  if (count === 0 && canUndo) return 'cleared';
  if (count > 0 && hover) return 'open';
  if (count > 0 || drag.offer) return 'resting';
  return 'hidden';
}

export function stripHeight(state: StripState): number {
  if (state === 'hidden') return 0;
  if (state === 'open' || state === 'target') return EXPANDED;
  return STRIP;
}

export function countLabel(count: number): string {
  return count === 1 ? t('shelf.item') : t('shelf.items', { count });
}
