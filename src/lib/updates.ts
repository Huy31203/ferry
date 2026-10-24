import { formatBytes } from './format';
import { t, type StringKey } from './i18n';
import type { UpdateView } from './svc';

export type UpdateButton = 'check' | 'install' | 'later' | 'skip' | 'retry' | 'release';

export interface UpdateText {
  line: StringKey;
  vars: Record<string, string | number>;
  buttons: UpdateButton[];
  /** Release notes, as plain text, for an offered update. */
  notes: string | null;
  /** Something is under way: no button works. */
  busy: boolean;
}

/** What About and updates says and offers for an update state. */
export function updateText(view: UpdateView): UpdateText {
  const state = view.state;
  const plain = { vars: {}, notes: null, busy: false };
  switch (state.kind) {
    case 'idle':
      return { ...plain, line: 'svc.updates.idle', buttons: ['check'] };
    case 'checking':
      return { ...plain, line: 'svc.updates.checking', buttons: [], busy: true };
    case 'up_to_date':
      return { ...plain, line: 'svc.updates.up_to_date', buttons: ['check'] };
    case 'available':
      return {
        ...plain,
        line: 'svc.updates.available',
        vars: { version: state.version },
        buttons: view.skipped ? ['install', 'release'] : ['install', 'later', 'skip', 'release'],
        notes: state.notes || t('svc.updates.no_notes'),
      };
    case 'downloading': {
      const known = state.total !== null && state.total > 0;
      const percent = known
        ? Math.min(100, Math.round((state.received / (state.total as number)) * 100))
        : 0;
      return {
        ...plain,
        line: known ? 'svc.updates.downloading' : 'svc.updates.downloading_bytes',
        vars: known
          ? { version: state.version, percent: `${percent} %` }
          : { version: state.version, size: formatBytes(state.received) },
        buttons: [],
        busy: true,
      };
    }
    case 'ready':
      return {
        ...plain,
        line: 'svc.updates.ready',
        vars: { version: state.version },
        buttons: ['install', 'later'],
      };
    case 'installing':
      return {
        ...plain,
        line: 'svc.updates.installing',
        vars: { version: state.version },
        buttons: [],
        busy: true,
      };
    case 'failed':
      return {
        ...plain,
        line: `svc.updates.failed.${state.phase}`,
        vars: { why: state.reason },
        buttons: ['retry'],
      };
  }
}
