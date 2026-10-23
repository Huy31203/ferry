import type { JobState, JobUpdate } from './ipc';

/** A 2880×1800 PNG clipboard card in `state`, for tests; `extra` overrides any field. */
export function card(state: JobState, extra: Partial<JobUpdate> = {}): JobUpdate {
  return {
    id: 'a',
    state,
    name: 'shot',
    ext: 'png',
    before: 2_000_000,
    after: null,
    detail: '',
    thumb_path: '/b/shot.png',
    source: 'clipboard',
    can_restore: false,
    can_retry: false,
    output: '/w/shot.png',
    dims_before: [2880, 1800],
    dims_after: null,
    format: 'png',
    converted: false,
    can_edit: true,
    edits: {
      aspect: null,
      framing: 'centre',
      size: null,
      scale: null,
      format: null,
      compression: null,
      fit_under: null,
      level: 'normal',
    },
    note: null,
    source_format: 'png',
    edit_hint: null,
    ...extra,
  };
}
