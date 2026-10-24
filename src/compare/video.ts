// stream A: video. How the result's player keeps pace with the original's.

/** How much longer the original is than the result (2 when the result was sped up 2×); 1 when unknown. */
export function ratio(originalMs: number | null, nowMs: number | null): number {
  if (!originalMs || !nowMs) return 1;
  return originalMs / nowMs;
}

/** Where the result's player is when the original's is at `leaderTime`. */
export function followerTime(leaderTime: number, r: number): number {
  return leaderTime / r;
}

/** The result plays at the original's pace: slower when it is the shorter one. */
export function followerRate(r: number): number {
  return 1 / r;
}

/** One frame forward or back, within the clip; 30 fps when unknown. */
export function frameStep(time: number, fps: number | null, dir: 1 | -1, duration: number): number {
  const next = time + dir / (fps || 30);
  return Math.min(duration, Math.max(0, next));
}

/** The two players are more than one frame apart. */
export function drifted(a: number, b: number, fps: number | null): boolean {
  return Math.abs(a - b) > 1 / (fps || 30);
}
