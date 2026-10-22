import type { JobState, JobUpdate } from './ipc';

export type RailAction = 'copy' | 'reveal' | 'restore' | 'aggressive';

/** Waiting for or running its encoder: no rail, and the close button cancels. */
export function isBusy(state: JobState): boolean {
  return state.kind === 'queued' || state.kind === 'optimising';
}

/** Finished cards fade out after `floating.auto_hide_secs`; busy and error cards stay (§5.5). */
export function autoHides(state: JobState): boolean {
  return ['done', 'already_optimal', 'larger', 'restored'].includes(state.kind);
}

/** The rail's buttons for a card, in order (§7 card anatomy). */
export function railActions(card: JobUpdate): RailAction[] {
  if (!autoHides(card.state) || card.output === null) return [];
  const actions: RailAction[] = ['copy', 'reveal'];
  if (card.state.kind === 'done' && card.can_restore) actions.push('restore');
  if (card.state.kind !== 'restored') actions.push('aggressive');
  return actions;
}

/** Whether the card's file can be dragged out. */
export function canDrag(card: JobUpdate): boolean {
  return autoHides(card.state) && card.output !== null;
}

/** Replace the card with the same id where it is, or add a new one at the end. */
export function upsert(cards: JobUpdate[], card: JobUpdate): JobUpdate[] {
  const at = cards.findIndex((c) => c.id === card.id);
  if (at === -1) return [...cards, card];
  return cards.map((c, i) => (i === at ? card : c));
}

/** The cards "Drag all" carries: every finished one. */
export function draggable(cards: JobUpdate[]): JobUpdate[] {
  return cards.filter(canDrag);
}

/** The files "Drag all" carries: every finished card's output. */
export function dragAllFiles(cards: JobUpdate[]): string[] {
  return draggable(cards).map((c) => c.output as string);
}
