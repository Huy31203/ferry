// Stream C: the Watched folders pane. Types mirror ferry_core::watch; the pure
// helpers keep the folder list edits out of the components.
import { t, type StringKey } from './i18n';
import type { WatchedFolder } from './settings';

export type DirState =
  | { kind: 'watching' }
  | { kind: 'missing' }
  | { kind: 'paused' }
  | { kind: 'denied' }
  | { kind: 'failed'; reason: string };
export interface DirStatus {
  path: string;
  state: DirState;
}
export interface WatchStatus {
  captures: DirStatus[];
  folders: DirStatus[];
}
/** Which media types this build can handle (src-tauri/src/watch/hooks.rs). */
export interface Support {
  image: boolean;
  video: boolean;
  audio: boolean;
  pdf: boolean;
}

export const MEDIA = ['image', 'video', 'audio', 'pdf'] as const;
export type Media = (typeof MEDIA)[number];

/** Mirrors ferry_core::settings::WatchedFolder::default() with `path`. */
export function newFolder(path: string): WatchedFolder {
  return {
    path,
    types: { image: true, video: true, audio: false, pdf: false },
    show_results: true,
    ignore: [],
    max_files_at_once: 10,
    paused: false,
  };
}

export function addFolder(
  folders: WatchedFolder[],
  path: string,
): { folders: WatchedFolder[]; duplicate: boolean } {
  if (folders.some((f) => f.path === path)) return { folders, duplicate: true };
  return { folders: [...folders, newFolder(path)], duplicate: false };
}

/** Fields the pane does not know (a later stream's) stay as they were. */
export function updateFolder(
  folders: WatchedFolder[],
  index: number,
  patch: Partial<WatchedFolder>,
): WatchedFolder[] {
  return folders.map((f, i) => (i === index ? { ...f, ...patch } : f));
}

export function removeFolder(folders: WatchedFolder[], index: number): WatchedFolder[] {
  return folders.filter((_, i) => i !== index);
}

/** Split on commas and newlines; trim; drop empties and repeats. */
export function parseIgnore(text: string): string[] {
  const rules = text
    .split(/[,\n]/)
    .map((rule) => rule.trim())
    .filter((rule) => rule !== '');
  return [...new Set(rules)];
}

export function formatIgnore(list: string[]): string {
  return list.join(', ');
}

export function statusFor(status: WatchStatus | null, path: string): DirState | null {
  return status?.folders.find((f) => f.path === path)?.state ?? null;
}

export function statusText(state: DirState, capture: boolean): string {
  switch (state.kind) {
    case 'watching':
      return t('watch.status.watching');
    case 'missing':
      return t(capture ? 'watch.status.not_yet' : 'watch.status.missing');
    case 'paused':
      return t('watch.status.paused');
    case 'denied':
      return t('watch.status.denied');
    case 'failed':
      return t('watch.status.failed', { reason: state.reason });
  }
}

export function statusTone(state: DirState): 'ok' | 'warn' | 'off' | 'error' {
  switch (state.kind) {
    case 'watching':
      return 'ok';
    case 'missing':
      return 'warn';
    case 'paused':
      return 'off';
    case 'denied':
    case 'failed':
      return 'error';
  }
}

/** The last part of a folder path, for the screen-reader labels. */
export function folderName(path: string): string {
  const parts = path.split(/[\\/]/).filter((part) => part !== '');
  return parts[parts.length - 1] ?? path;
}

export function mediaLabel(media: Media): StringKey {
  return `watch.type.${media}`;
}
