import type { StringKey } from './i18n';
import type { FolderInfo, SyncStatus } from './svc';

const sameDay = (a: Date, b: Date) =>
  a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();

/** The status line for Settings, or null while sync is off. `now` is ms since the epoch. */
export function syncLine(
  status: SyncStatus,
  now: number,
): { key: StringKey; vars: Record<string, string | number> } | null {
  switch (status.kind) {
    case 'off':
      return null;
    case 'starting':
      return { key: 'svc.sync.status.starting', vars: {} };
    case 'synced': {
      const when = new Date(status.at);
      const time = sameDay(when, new Date(now)) ? when.toLocaleTimeString() : when.toLocaleString();
      if (status.devices === 0) return { key: 'svc.sync.status.synced', vars: { time } };
      if (status.devices === 1) return { key: 'svc.sync.status.synced_with_one', vars: { time } };
      return { key: 'svc.sync.status.synced_with', vars: { time, count: status.devices } };
    }
    case 'folder_missing':
      return { key: 'svc.sync.status.missing', vars: {} };
    case 'newer_format':
      return { key: 'svc.sync.status.newer', vars: { device: status.device } };
    case 'failed':
      return { key: 'svc.sync.status.failed', vars: { why: status.reason } };
  }
}

/** Other computers already keep settings in the folder: the user decides whose values win. */
export function needsJoin(info: FolderInfo): boolean {
  return info.devices.length > 0;
}

/** The computers' names (newest first, as Rust sends them) and when the newest wrote. */
export function joinSummary(info: FolderInfo): { devices: string; time: string } {
  const newest = Math.max(...info.devices.map((device) => device.written_at));
  return {
    devices: info.devices.map((device) => device.name).join(', '),
    time: new Date(newest).toLocaleString(),
  };
}
