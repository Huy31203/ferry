import type { JobState, JobUpdate } from './ipc';
import { DEFAULT_RAIL_TOOLS, MAX_COMPACT_TOOLS, MAX_TOOLS, RAIL_TOOLS } from './rail';

/** A catalog tool id (`rail.ts`) or `'more'`. Rail A is the default set (spec §5.1). */
export type RailAction = string;

/** Waiting for or running its encoder: no rail, and the close button cancels. */
export function isBusy(state: JobState): boolean {
  return state.kind === 'queued' || state.kind === 'optimising';
}

/** Finished cards fade out after `floating.auto_hide_secs`; busy and error cards stay (§5.5). */
export function autoHides(state: JobState): boolean {
  return ['done', 'already_optimal', 'larger', 'restored'].includes(state.kind);
}

/** The rail's buttons for a finished card, in the user's order, then the ⋯ menu. Unknown ids are skipped; a compact card shows fewer. */
export function railActions(
  card: JobUpdate,
  compact = false,
  tools: string[] = DEFAULT_RAIL_TOOLS,
): RailAction[] {
  if (!autoHides(card.state) || card.output === null) return [];
  const limit = compact ? MAX_COMPACT_TOOLS : MAX_TOOLS;
  const actions: RailAction[] = [];
  for (const id of tools) {
    const tool = RAIL_TOOLS.find((candidate) => candidate.id === id);
    if (!tool || (compact && !tool.compact) || (tool.applies && !tool.applies(card))) continue;
    if (actions.length === limit) break;
    actions.push(id === 'downscale' && card.source_format === 'pdf' ? 'dpi' : id); // sE pdf-formats: a PDF has DPI, not a scale
  }
  actions.push('more');
  return actions;
}

/** Rail buttons that work on the original, so `can_edit` turns them off. */
export function needsOriginal(action: RailAction): boolean {
  return RAIL_TOOLS.find((tool) => tool.id === (action === 'dpi' ? 'downscale' : action))?.needsOriginal ?? false;
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

/** The stack without the card with `id` (closed here, or by the app). */
export function remove(cards: JobUpdate[], id: string): JobUpdate[] {
  return cards.filter((c) => c.id !== id);
}

/** The cards "Drag all" carries: every finished one. */
export function draggable(cards: JobUpdate[]): JobUpdate[] {
  return cards.filter(canDrag);
}

/** The files "Drag all" carries: every finished card's output. */
export function dragAllFiles(cards: JobUpdate[]): string[] {
  return draggable(cards).map((c) => c.output as string);
}

/** A card tool that needs the keyboard (or, for the slider, Esc). */
export type Tool = 'rename' | 'crop' | 'fit' | 'pages' | 'slider' | 'speed'; // 'pages': sE pdf-formats, 'speed': stream A: video

/** A tool shown as a popover: a click elsewhere closes it. */
export function isPopover(kind: Tool): boolean {
  return kind === 'crop' || kind === 'fit' || kind === 'pages' || kind === 'speed';
}

/** The one tool open in the stack, and the card it belongs to. */
export interface OpenTool {
  id: string;
  kind: Tool;
}

/** `tool`, or null once its card is gone or busy: the stack must not stay key for a card that left. */
export function openTool(tool: OpenTool | null, cards: JobUpdate[]): OpenTool | null {
  if (tool === null) return null;
  const owner = cards.find((c) => c.id === tool.id);
  return owner && canDrag(owner) ? tool : null;
}

/** The card the hover hotkeys act on (spec §4.5): the idle card under the pointer,
 *  unless a tool has the keyboard or the setting is off. */
export function hoverTarget(
  hovered: string | null,
  cards: JobUpdate[],
  tool: OpenTool | null,
  enabled: boolean,
): string | null {
  if (!enabled || tool !== null || hovered === null) return null;
  const card = cards.find((c) => c.id === hovered);
  return card && canDrag(card) ? card.id : null;
}
