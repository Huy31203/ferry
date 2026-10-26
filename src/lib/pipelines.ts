// Stream F: pipelines. Types mirror ferry_core::{pipeline_def, pipeline_steps,
// pipeline_plan, pipeline_text, pipeline_test}; the pure helpers keep the
// library edits out of the components.
import { t, type StringKey } from './i18n';
import { MEDIA, folderName, type Media } from './watch';

export { MEDIA };
export type { Media };

export type Params = Record<string, unknown>;

/** Fields the editor does not know (a newer Ferry's) ride along in `...step`. */
export interface Step {
  kind: string;
  params: Params;
  [extra: string]: unknown;
}

export interface Pipeline {
  id: number;
  name: string;
  media: Media;
  steps: Step[];
  hide_result: boolean;
  [extra: string]: unknown;
}

export type PerMedia = Record<Media, number | null>;

export interface Zone {
  id: number;
  name: string;
  icon: string;
  pipelines: PerMedia;
}

export interface Pipelines {
  library: Pipeline[];
  clipboard: PerMedia;
  drop_zone: PerMedia;
  zones: Zone[];
}

export type ParamControl =
  | { kind: 'number'; min: number; max: number; step: number }
  | { kind: 'choice'; options: { value: string; label: string }[] }
  | { kind: 'switch' }
  | { kind: 'text' }
  | { kind: 'template' }
  | { kind: 'folder' };

export interface ParamView {
  name: string;
  label: string;
  control: ParamControl;
  required: boolean;
  shown_when: { param: string; values: string[] } | null;
}

export interface StepKindView {
  id: string;
  media: Media;
  label: string;
  phase: 'transform' | 'output' | 'control';
  params: ParamView[];
  defaults: Params;
}

export interface Issue {
  step: number;
  message: string;
}

export interface LineError {
  line: number;
  message: string;
}

export interface StepReport {
  step: number;
  caption: string;
  bytes: number | null;
  dims: [number, number] | null;
  millis: number;
  error: string | null;
  note: string | null;
}

export interface PipeMeta {
  media: Media[];
  last_run: Record<number, string>;
}

export interface Checked {
  issues: Issue[];
  captions: string[];
}

/** A new pipeline with the next id (largest + 1). */
export function newPipeline(
  lib: Pipelines,
  media: Media,
  name: string,
  steps: Step[] = [],
): { lib: Pipelines; id: number } {
  const id = lib.library.reduce((max, p) => Math.max(max, p.id), 0) + 1;
  const pipeline: Pipeline = { id, name, media, steps, hide_result: false };
  return { lib: { ...lib, library: [...lib.library, pipeline] }, id };
}

/** A copy of pipeline `id` under a new id and `name`, at the end of the library. */
export function duplicate(lib: Pipelines, id: number, name: string): { lib: Pipelines; id: number } {
  const source = lib.library.find((p) => p.id === id);
  if (!source) return { lib, id };
  const copy = { ...structuredClone(source), name };
  const next = newPipeline(lib, source.media, name, copy.steps);
  next.lib.library[next.lib.library.length - 1] = { ...copy, id: next.id };
  return next;
}

const without = (slots: PerMedia, id: number): PerMedia =>
  Object.fromEntries(Object.entries(slots).map(([m, v]) => [m, v === id ? null : v])) as PerMedia;

/** Remove a pipeline and every assignment to it. */
export function removePipeline(lib: Pipelines, id: number): Pipelines {
  return {
    library: lib.library.filter((p) => p.id !== id),
    clipboard: without(lib.clipboard, id),
    drop_zone: without(lib.drop_zone, id),
    zones: lib.zones.map((z) => ({ ...z, pipelines: without(z.pipelines, id) })),
  };
}

export type Usage =
  | { kind: 'clipboard' | 'drop_zone'; media: Media }
  | { kind: 'zone'; media: Media; zone: string }
  | { kind: 'folder'; folder: string };

/** Where pipeline `id` runs: the clipboard, the drop zone, the zones in list order, then watched folders. */
export function usage(lib: Pipelines, id: number, folders: { path: string; pipeline: number | null }[] = []): Usage[] {
  const out: Usage[] = [];
  for (const media of MEDIA) if (lib.clipboard[media] === id) out.push({ kind: 'clipboard', media });
  for (const media of MEDIA) if (lib.drop_zone[media] === id) out.push({ kind: 'drop_zone', media });
  for (const zone of lib.zones)
    for (const media of MEDIA)
      if (zone.pipelines[media] === id) out.push({ kind: 'zone', media, zone: zone.name });
  for (const f of folders) if (f.pipeline === id) out.push({ kind: 'folder', folder: folderName(f.path) });
  return out;
}

/** "Copied images · Zone “WebP”", or the "not assigned" text. */
export function usageLabel(list: Usage[]): string {
  if (list.length === 0) return t('pipe.assign.none');
  const parts = list.map((u) => {
    if (u.kind === 'drop_zone') return t('pipe.assign.drop_zone');
    if (u.kind === 'zone') return t('pipe.assign.zone', { name: u.zone });
    if (u.kind === 'folder') return t('pipe.assign.folder', { name: u.folder });
    return t('pipe.assign.clipboard', { media: t(`pipe.media.${u.media}` as StringKey).toLowerCase() });
  });
  return [...new Set(parts)].join(' · ');
}

// ---- steps ----

/** Move the step at `from` so it ends at `to`. */
export function moveStep(steps: Step[], from: number, to: number): Step[] {
  if (from === to || from < 0 || from >= steps.length || to < 0 || to >= steps.length) return steps;
  const next = [...steps];
  next.splice(to, 0, next.splice(from, 1)[0]);
  return next;
}

/** A new step of `kind` with its default params, at `at` or at the end. */
export function addStep(steps: Step[], kind: StepKindView, at?: number): Step[] {
  const step: Step = { kind: kind.id, params: structuredClone(kind.defaults) };
  const next = [...steps];
  next.splice(at ?? next.length, 0, step);
  return next;
}

export function removeStep(steps: Step[], i: number): Step[] {
  return steps.filter((_, n) => n !== i);
}

/** Set a param of step `i`; `undefined` deletes it. */
export function setParam(steps: Step[], i: number, name: string, value: unknown | undefined): Step[] {
  return steps.map((step, n) => {
    if (n !== i) return step;
    const params = { ...step.params };
    if (value === undefined) delete params[name];
    else params[name] = value;
    return { ...step, params };
  });
}

// ---- text mode ----

/** The text of `token` the cursor is in, and where it starts. */
function tokenAt(line: string, col: number): { start: number; text: string } {
  const before = line.slice(0, col);
  const start = Math.max(before.lastIndexOf(' '), before.lastIndexOf('\t')) + 1;
  return { start, text: before.slice(start) };
}

/** What the text editor offers at `col` of `line`: step ids in the first word, then the step's settings not yet on the line (each ending in `=`), then a choice setting's values. */
export function suggest(line: string, col: number, kinds: StepKindView[]): string[] {
  const { start, text } = tokenAt(line, col);
  if (start === 0) return kinds.map((k) => k.id).filter((id) => id.startsWith(text));
  const kind = kinds.find((k) => k.id === line.trimStart().split(/\s+/)[0]);
  if (!kind) return [];
  const equals = text.indexOf('=');
  if (equals !== -1) {
    const param = kind.params.find((p) => p.name === text.slice(0, equals));
    if (param?.control.kind !== 'choice') return [];
    const typed = text.slice(equals + 1);
    return param.control.options.map((o) => o.value).filter((v) => v.startsWith(typed));
  }
  const used = new Set(
    line
      .split(/\s+/)
      .slice(1)
      .filter((word) => word.includes('=') && word !== text)
      .map((word) => word.slice(0, word.indexOf('='))),
  );
  return kind.params
    .map((p) => p.name)
    .filter((name) => !used.has(name) && name.startsWith(text))
    .map((name) => `${name}=`);
}

/** `line` with the token at `col` replaced by `suggestion`, and the cursor after it. A value or a step id is followed by a space. */
export function applySuggestion(line: string, col: number, suggestion: string): { line: string; col: number } {
  const { start, text } = tokenAt(line, col);
  const equals = text.indexOf('=');
  const prefix = equals !== -1 && !suggestion.endsWith('=') ? text.slice(0, equals + 1) : '';
  const done = suggestion + (suggestion.endsWith('=') ? '' : ' ');
  const head = line.slice(0, start) + prefix + done;
  return { line: head + line.slice(col), col: head.length };
}

// ---- assignments ----

export type Slot = { kind: 'clipboard' } | { kind: 'drop_zone' } | { kind: 'zone'; zone: number };

/** Make pipeline `id` (or none) the one that runs in `slot` for `media`. */
export function assign(lib: Pipelines, slot: Slot, media: Media, id: number | null): Pipelines {
  const put = (slots: PerMedia): PerMedia => ({ ...slots, [media]: id });
  if (slot.kind === 'clipboard') return { ...lib, clipboard: put(lib.clipboard) };
  if (slot.kind === 'drop_zone') return { ...lib, drop_zone: put(lib.drop_zone) };
  return { ...lib, zones: lib.zones.map((z) => (z.id === slot.zone ? { ...z, pipelines: put(z.pipelines) } : z)) };
}

// ---- preset zones ----

export const ZONE_ICONS = ['image', 'copy', 'folder', 'zap', 'globe', 'star'] as const;

/** A new zone with the next id, an image icon and nothing assigned. */
export function newZone(lib: Pipelines, name: string): { lib: Pipelines; id: number } {
  const id = lib.zones.reduce((max, z) => Math.max(max, z.id), 0) + 1;
  const zone: Zone = { id, name, icon: 'image', pipelines: { image: null, video: null, audio: null, pdf: null } };
  return { lib: { ...lib, zones: [...lib.zones, zone] }, id };
}

export function removeZone(lib: Pipelines, id: number): Pipelines {
  return { ...lib, zones: lib.zones.filter((z) => z.id !== id) };
}

/** Move the zone at `from` so it ends at `to`. */
export function moveZone(lib: Pipelines, from: number, to: number): Pipelines {
  if (from === to || from < 0 || from >= lib.zones.length || to < 0 || to >= lib.zones.length) return lib;
  const zones = [...lib.zones];
  zones.splice(to, 0, zones.splice(from, 1)[0]);
  return { ...lib, zones };
}

/** Whether a tile takes the dragged files, and the line it shows: its pipeline's caption, or why it has none. */
export function tileState(
  tile: { pipelines: PerMedia; captions: Partial<Record<Media, string>> },
  dragged: Media[],
): { enabled: boolean; caption: string } {
  const candidates = dragged.length > 0 ? dragged : MEDIA;
  const hit = candidates.find((m) => tile.pipelines[m] !== null);
  if (hit) return { enabled: true, caption: tile.captions[hit] ?? '' };
  const media = t(`pipe.media.one.${candidates[0]}` as StringKey);
  return { enabled: false, caption: t('pipe.zone.none', { media }) };
}
