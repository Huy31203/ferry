export interface Range {
  start: number;
  end: number;
  padTop: number;
  padBottom: number;
}

/** The rows of a fixed-height list to draw for a scroll position, plus `overscan` each side. */
export function visibleRange(
  scrollTop: number,
  viewport: number,
  rowHeight: number,
  count: number,
  overscan = 6,
): Range {
  const start = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const end = Math.min(count, Math.ceil((scrollTop + viewport) / rowHeight) + overscan);
  return { start, end, padTop: start * rowHeight, padBottom: Math.max(0, count - end) * rowHeight };
}
