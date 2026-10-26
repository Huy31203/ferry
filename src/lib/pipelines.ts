// Stream F: pipelines. Types mirror ferry_core::{pipeline_def, pipeline_steps,
// pipeline_plan, pipeline_text, pipeline_test}; the pure helpers keep the
// library edits out of the components.
import { t, type StringKey } from './i18n';
import { MEDIA, type Media } from './watch';

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
  | { kind: 'zone'; media: Media; zone: string };

/** Where pipeline `id` runs: the clipboard, the drop zone, then the zones in list order. */
export function usage(lib: Pipelines, id: number): Usage[] {
  const out: Usage[] = [];
  for (const media of MEDIA) if (lib.clipboard[media] === id) out.push({ kind: 'clipboard', media });
  for (const media of MEDIA) if (lib.drop_zone[media] === id) out.push({ kind: 'drop_zone', media });
  for (const zone of lib.zones)
    for (const media of MEDIA)
      if (zone.pipelines[media] === id) out.push({ kind: 'zone', media, zone: zone.name });
  return out;
}

/** "Copied images · Zone “WebP”", or the "not assigned" text. */
export function usageLabel(list: Usage[]): string {
  if (list.length === 0) return t('pipe.assign.none');
  const parts = list.map((u) => {
    if (u.kind === 'drop_zone') return t('pipe.assign.drop_zone');
    if (u.kind === 'zone') return t('pipe.assign.zone', { name: u.zone });
    return t('pipe.assign.clipboard', { media: t(`pipe.media.${u.media}` as StringKey).toLowerCase() });
  });
  return [...new Set(parts)].join(' · ');
}
