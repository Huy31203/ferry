import type { StringKey } from '../lib/i18n';

export const LEVELS = [0, 1, 3, 5, 7, 9] as const;
export const VOLUMES = ['10M', '100M', '650M', '700M', '4092M', '4480M'] as const;

export type Form = { name: string; password: string; again: string; split: string };

/** The first problem that keeps OK disabled; the volume size is checked by Rust. */
export function checkForm(f: Form): StringKey | null {
  const name = f.name.trim();
  if (!name || name.includes('/')) return 'archive.error.name';
  if (f.password !== f.again) return 'archive.mismatch';
  return null;
}
