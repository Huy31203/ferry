import { describe, expect, it } from 'vitest';
import {
  addFolder,
  folderName,
  formatIgnore,
  newFolder,
  parseIgnore,
  pipelineId,
  removeFolder,
  statusFor,
  statusText,
  statusTone,
  updateFolder,
} from './watch';

describe('watched folders', () => {
  it('adds a folder with the Rust defaults and refuses a duplicate', () => {
    const first = addFolder([], '~/Downloads');
    expect(first.folders).toEqual([newFolder('~/Downloads')]);
    expect(first.folders[0]).toEqual({
      path: '~/Downloads',
      types: { image: true, video: true, audio: false, pdf: false },
      show_results: true,
      ignore: [],
      max_files_at_once: 10,
      paused: false,
      pipeline: null,
    });
    expect(addFolder(first.folders, '~/Downloads')).toEqual({
      folders: first.folders,
      duplicate: true,
    });
  });

  it('updates one folder and keeps unknown fields', () => {
    const withUnknown = [{ ...newFolder('~/A'), later: 'x' }] as never;
    const next = updateFolder(withUnknown, 0, { paused: true });
    expect(next[0]).toMatchObject({ paused: true, later: 'x' });
  });

  it('reads the pipeline picker', () => {
    expect(pipelineId('')).toBeNull();
    expect(pipelineId('7')).toBe(7);
  });

  it('names a folder by its last part', () => {
    expect(folderName('~/Downloads')).toBe('Downloads');
    expect(folderName('C:\\Users\\me\\Pictures\\')).toBe('Pictures');
    expect(folderName('~')).toBe('~');
  });

  it('removes by index', () => {
    expect(removeFolder([newFolder('a'), newFolder('b')], 0).map((f) => f.path)).toEqual(['b']);
  });

  it('parses ignore rules', () => {
    expect(parseIgnore(' *.tmp, draft-*\n*.tmp ,, ')).toEqual(['*.tmp', 'draft-*']);
    expect(formatIgnore(['*.tmp', 'draft-*'])).toBe('*.tmp, draft-*');
  });

  it('finds a folder status by its stored path and words it', () => {
    const status = { captures: [], folders: [{ path: '~/A', state: { kind: 'missing' as const } }] };
    expect(statusFor(status, '~/A')).toEqual({ kind: 'missing' });
    expect(statusFor(status, '~/B')).toBeNull();
    expect(statusText({ kind: 'missing' }, false)).toBe('Missing — will retry');
    expect(statusText({ kind: 'missing' }, true)).toBe('Appears after your first capture');
    expect(statusText({ kind: 'failed', reason: 'x' }, false)).toBe('Not watching: x');
    expect(statusTone({ kind: 'watching' })).toBe('ok');
    expect(statusTone({ kind: 'denied' })).toBe('error');
    expect(statusTone({ kind: 'paused' })).toBe('off');
  });
});
