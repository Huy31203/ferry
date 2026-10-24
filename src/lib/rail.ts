import { t, type StringKey } from './i18n';
import type { JobUpdate } from './ipc';

/** A card tool the rail can show. Another stream adds one entry here and one button block in Card.svelte. */
export interface RailTool {
  id: string;
  label: StringKey;
  labelWindows?: StringKey;
  needsOriginal: boolean;
  /** Whether a compact card may show it. */
  compact: boolean;
  /** Whether the tool applies to this finished card. Default: always. */
  applies?: (card: JobUpdate) => boolean;
}

/** In built-in order: the DOM order of the buttons in Card.svelte. */
export const RAIL_TOOLS: RailTool[] = [
  { id: 'downscale', label: 'svc.rail.tool.downscale', needsOriginal: true, compact: true },
  { id: 'compression', label: 'svc.rail.tool.compression', needsOriginal: true, compact: false },
  { id: 'crop', label: 'svc.rail.tool.crop', needsOriginal: true, compact: true },
  {
    id: 'restore',
    label: 'svc.rail.tool.restore',
    needsOriginal: false,
    compact: true,
    applies: (card) => card.state.kind === 'done' && card.can_restore,
  },
  { id: 'compare', label: 'svc.rail.tool.compare', needsOriginal: true, compact: false },
  { id: 'copy', label: 'svc.rail.tool.copy', needsOriginal: false, compact: true },
  {
    id: 'reveal',
    label: 'svc.rail.tool.reveal.mac',
    labelWindows: 'svc.rail.tool.reveal.windows',
    needsOriginal: false,
    compact: true,
  },
  { id: 'aggressive', label: 'svc.rail.tool.aggressive', needsOriginal: true, compact: true },
  { id: 'rename', label: 'svc.rail.tool.rename', needsOriginal: false, compact: true },
  { id: 'preview', label: 'svc.rail.tool.preview', needsOriginal: false, compact: true },
  { id: 'fit', label: 'svc.rail.tool.fit', needsOriginal: true, compact: true },
];

/** The Rust `DEFAULT_RAIL`. */
export const DEFAULT_RAIL_TOOLS: string[] = ['downscale', 'compression', 'crop', 'restore', 'compare', 'copy'];

export const MAX_TOOLS = 6;
export const MAX_COMPACT_TOOLS = 4;

const find = (id: string) => RAIL_TOOLS.find((tool) => tool.id === id);

/** The catalog label, or "Not in this version" for an id this build does not know. */
export function toolLabel(id: string, mac: boolean): string {
  const tool = find(id);
  if (!tool) return t('svc.rail.unknown', { id });
  return t(!mac && tool.labelWindows ? tool.labelWindows : tool.label);
}

/** How many of `tools` this build can draw. */
export function knownCount(tools: string[]): number {
  return tools.filter((id) => find(id)).length;
}

/** Catalog tools not in `tools`, in catalog order. */
export function addable(tools: string[]): RailTool[] {
  return RAIL_TOOLS.filter((tool) => !tools.includes(tool.id));
}

/** One row per id in the editor; unknown ids are kept and shown greyed. */
export function editorRows(tools: string[]): { id: string; known: boolean }[] {
  return tools.map((id) => ({ id, known: find(id) !== undefined }));
}

/** A new array with the item at `from` moved to index `to`. */
export function moveItem<T>(list: T[], from: number, to: number): T[] {
  const next = [...list];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}

/** The index a dragged row lands at: how many row midpoints lie above `y`. */
export function dropIndex(y: number, midpoints: number[]): number {
  return midpoints.filter((mid) => mid < y).length;
}
