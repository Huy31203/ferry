import { t, type StringKey } from './i18n';
import type { Summary, ZoneView } from './drop-ipc';

export interface ZoneText {
  label: string;
  sub: string | null;
  keep: string | null;
  dim: boolean;
}

type Noun = 'image' | 'video' | 'audio' | 'pdf' | 'folder' | 'file';

function count(noun: Noun, n: number): string {
  return t(`drop.count.${noun}.${n === 1 ? 'one' : 'other'}` as StringKey, { n });
}

/** "3 images, 1 video": only the kinds present, then files not read yet as plain files. */
export function kindsText(s: Summary): string {
  const read = s.images + s.videos + s.audio + s.pdfs + s.unsupported;
  const parts: [Noun, number][] = [
    ['image', s.images],
    ['video', s.videos],
    ['audio', s.audio],
    ['pdf', s.pdfs],
    ['folder', s.folders],
    ['file', Math.max(0, s.files - read)],
  ];
  return parts
    .filter(([, n]) => n > 0)
    .map(([noun, n]) => count(noun, n))
    .join(', ');
}

export function zoneText(view: ZoneView): ZoneText {
  const s = view.summary;
  const unsupported = view.state === 'unsupported';
  const label = unsupported
    ? s?.unsupported_ext
      ? t('drop.zone.unsupported', { ext: s.unsupported_ext })
      : t('drop.zone.unsupported_any')
    : t('drop.zone.label');
  let sub: string | null = null;
  if (s) {
    if (view.opens_batch) {
      const what = [
        s.files > 0 ? count('file', s.files) : '',
        s.folders > 0 ? count('folder', s.folders) : '',
      ].filter(Boolean);
      sub = t('drop.zone.opens_batch', { what: what.join(', ') });
    } else {
      sub = kindsText(s) || null;
    }
  }
  return {
    label,
    sub,
    keep: view.keep_original ? t('drop.zone.keep') : null,
    dim: unsupported,
  };
}

/** The tip that a Ctrl tap opens the preset zones: only while there are zones to open, and the zone is not busy with a drop. */
export function presetsHint(view: ZoneView, zones: number): string | null {
  const showing = view.state === 'idle' || view.state === 'hover';
  return zones > 0 && showing ? t('pipe.zones.hint_tap') : null;
}
