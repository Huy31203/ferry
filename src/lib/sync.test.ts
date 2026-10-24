import { describe, expect, it } from 'vitest';
import type { FolderInfo } from './svc';
import { joinSummary, needsJoin, syncLine } from './sync';

const at = new Date(2026, 9, 24, 10, 30).getTime();
const sameDay = new Date(2026, 9, 24, 18, 0).getTime();
const nextDay = new Date(2026, 9, 25, 9, 0).getTime();

describe('syncLine', () => {
  it('says nothing while sync is off', () => {
    expect(syncLine({ kind: 'off' }, sameDay)).toBeNull();
  });

  it('says syncing while it starts', () => {
    expect(syncLine({ kind: 'starting' }, sameDay)).toEqual({
      key: 'svc.sync.status.starting',
      vars: {},
    });
  });

  it('names the time, and how many other computers took part', () => {
    const time = new Date(at).toLocaleTimeString();
    expect(syncLine({ kind: 'synced', at, devices: 0 }, sameDay)).toEqual({
      key: 'svc.sync.status.synced',
      vars: { time },
    });
    expect(syncLine({ kind: 'synced', at, devices: 1 }, sameDay)).toEqual({
      key: 'svc.sync.status.synced_with_one',
      vars: { time },
    });
    expect(syncLine({ kind: 'synced', at, devices: 3 }, sameDay)).toEqual({
      key: 'svc.sync.status.synced_with',
      vars: { time, count: 3 },
    });
  });

  it('adds the date when the last sync was not today', () => {
    const line = syncLine({ kind: 'synced', at, devices: 0 }, nextDay);
    expect(line?.vars.time).toBe(new Date(at).toLocaleString());
  });

  it('reports a missing folder, a newer Ferry and a failure', () => {
    expect(syncLine({ kind: 'folder_missing' }, sameDay)?.key).toBe('svc.sync.status.missing');
    expect(syncLine({ kind: 'newer_format', device: "Huy's PC" }, sameDay)).toEqual({
      key: 'svc.sync.status.newer',
      vars: { device: "Huy's PC" },
    });
    expect(syncLine({ kind: 'failed', reason: 'disk full' }, sameDay)).toEqual({
      key: 'svc.sync.status.failed',
      vars: { why: 'disk full' },
    });
  });
});

describe('joining a folder', () => {
  const none: FolderInfo = { exists: true, devices: [], newer_format: false };
  const two: FolderInfo = {
    exists: true,
    newer_format: false,
    devices: [
      { device: 'd2', name: 'Studio Mac', os: 'macos', written_at: at },
      { device: 'd1', name: 'Desk PC', os: 'windows', written_at: at - 60_000 },
    ],
  };

  it('asks only when other computers already keep settings there', () => {
    expect(needsJoin(none)).toBe(false);
    expect(needsJoin({ ...none, devices: [two.devices[0]] })).toBe(true);
  });

  it('lists the computers newest first, with the newest time', () => {
    expect(joinSummary(two)).toEqual({
      devices: 'Studio Mac, Desk PC',
      time: new Date(at).toLocaleString(),
    });
  });
});
