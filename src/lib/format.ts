const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'] as const;

/** Decimal (1000-based) size: 2_400_000 → "2.4 MB"; one decimal below 100 units. */
export function formatBytes(bytes: number, locale?: string): string {
  let value = Math.max(0, bytes);
  let unit = 0;
  while (value >= 1000 && unit < UNITS.length - 1) {
    value /= 1000;
    unit += 1;
  }
  let digits = unit > 0 && value < 100 ? 1 : 0;
  let rounded = Number(value.toFixed(digits));
  if (rounded >= 1000 && unit < UNITS.length - 1) {
    rounded = 1;
    digits = 0;
    unit += 1;
  }
  const text = new Intl.NumberFormat(locale, { maximumFractionDigits: digits }).format(rounded);
  return `${text} ${UNITS[unit]}`;
}

/** Change from `before` to `after` as "−84%" / "+12%" / "0%" (U+2212 minus). */
export function savingPercent(before: number, after: number): string {
  if (before <= 0) return '0%';
  const change = Math.round((after / before - 1) * 100);
  if (change === 0) return '0%';
  return change < 0 ? `−${-change}%` : `+${change}%`;
}
