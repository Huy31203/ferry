import { describe, expect, it } from 'vitest';
import { drifted, followerRate, followerTime, frameStep, ratio } from './video';

describe('video compare', () => {
  it('relates the two durations', () => {
    expect(ratio(10_000, 5_000)).toBe(2);
    expect(ratio(null, 5_000)).toBe(1);
    expect(ratio(10_000, 0)).toBe(1);
  });

  it('keeps the result at the original pace', () => {
    expect(followerTime(4, 2)).toBe(2);
    expect(followerRate(2)).toBe(0.5);
  });

  it('steps a frame at a time within the clip', () => {
    expect(frameStep(1, 25, 1, 10)).toBeCloseTo(1.04);
    expect(frameStep(0, null, -1, 10)).toBe(0);
    expect(frameStep(9.99, 30, 1, 10)).toBe(10);
  });

  it('notices drift of more than a frame', () => {
    expect(drifted(1, 1.02, 30)).toBe(false);
    expect(drifted(1, 1.05, 30)).toBe(true);
  });
});
