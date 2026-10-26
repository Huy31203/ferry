import { t, type StringKey } from './i18n';
import type { JobUpdate } from './ipc';
import type { Media } from './watch';

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

/** A library pipeline the rail can run on a card (stream F). Its tool id is `pipeline:<id>`. */
export interface RailPipeline {
  id: number;
  name: string;
  media: Media;
}

/** In built-in order, the order of the "+" menu and the editor's list. */
export const RAIL_TOOLS: RailTool[] = [
  // stream B: audio (downscale resizes the cover, compression is the bitrate; no crop or compare)
  {
    id: 'downscale',
    label: 'svc.rail.tool.downscale',
    needsOriginal: true,
    compact: true,
    applies: (card) => !card.audio || (card.audio.has_cover && card.dims_before !== null),
  },
  {
    id: 'compression',
    label: 'svc.rail.tool.compression',
    needsOriginal: true,
    compact: false,
    // sE pdf-formats (TIFF output is lossless, so there is no quality to set)
    applies: (card) => (!card.audio || !card.audio.lossless) && card.format !== 'tiff',
  },
  { id: 'crop', label: 'svc.rail.tool.crop', needsOriginal: true, compact: true, applies: (card) => !card.audio },
  {
    id: 'restore',
    label: 'svc.rail.tool.restore',
    needsOriginal: false,
    compact: true,
    applies: (card) => card.state.kind === 'done' && card.can_restore,
  },
  { id: 'compare', label: 'svc.rail.tool.compare', needsOriginal: true, compact: false, applies: (card) => !card.audio },
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

export const pipelineToolId = (id: number) => `pipeline:${id}`;

/** The library pipeline a `pipeline:<id>` tool id names, if it is still there. */
export function pipelineFor(id: string, library: RailPipeline[]): RailPipeline | undefined {
  return library.find((pipeline) => pipelineToolId(pipeline.id) === id);
}

const isKnown = (id: string, library: RailPipeline[]) => find(id) !== undefined || pipelineFor(id, library) !== undefined;

/** The catalog label, a pipeline's name, or "Not in this version" for an id this build does not know. */
export function toolLabel(id: string, mac: boolean, library: RailPipeline[] = []): string {
  const pipeline = pipelineFor(id, library);
  if (pipeline) return pipeline.name;
  const tool = find(id);
  if (!tool) return t('svc.rail.unknown', { id });
  return t(!mac && tool.labelWindows ? tool.labelWindows : tool.label);
}

/** How many of `tools` this build can draw. */
export function knownCount(tools: string[], library: RailPipeline[] = []): number {
  return tools.filter((id) => isKnown(id, library)).length;
}

/** Catalog tools, then pipelines, that are not in `tools`. */
export function addable(tools: string[], library: RailPipeline[] = []): { id: string }[] {
  return [
    ...RAIL_TOOLS.filter((tool) => !tools.includes(tool.id)),
    ...library.map((pipeline) => ({ id: pipelineToolId(pipeline.id) })).filter((tool) => !tools.includes(tool.id)),
  ];
}

/** One row per id in the editor; unknown ids are kept and shown greyed. */
export function editorRows(tools: string[], library: RailPipeline[] = []): { id: string; known: boolean }[] {
  return tools.map((id) => ({ id, known: isKnown(id, library) }));
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
