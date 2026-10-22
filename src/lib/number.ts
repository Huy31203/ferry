/**
 * What a number field should save when the user leaves it: the typed value,
 * rounded and clamped to `min..max`, or null for "save nothing". A blank or
 * unreadable field must not become the minimum — for "Keep backups for" that
 * would delete originals at the next start.
 */
export function commitNumber(
  raw: string,
  badInput: boolean,
  current: number,
  min: number,
  max: number,
): number | null {
  if (badInput || raw.trim() === '') return null;
  const typed = Number(raw);
  if (!Number.isFinite(typed)) return null;
  const clamped = Math.min(max, Math.max(min, Math.round(typed)));
  return clamped === current ? null : clamped;
}
