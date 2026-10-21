import { describe, expect, it } from 'vitest';
import { formatBytes, savingPercent } from './format';

describe('formatBytes', () => {
  it.each([
    [0, '0 B'],
    [999, '999 B'],
    [1_000, '1 KB'],
    [1_500, '1.5 KB'],
    [380_000, '380 KB'],
    [2_400_000, '2.4 MB'],
    [12_345_678, '12.3 MB'],
    [999_999, '1 MB'],
    [1_234_567_890, '1.2 GB'],
    [-5, '0 B'],
  ])('%d bytes → %s', (bytes, expected) => {
    expect(formatBytes(bytes, 'en-US')).toBe(expected);
  });
});

describe('savingPercent', () => {
  it('shows savings with a real minus sign', () => {
    expect(savingPercent(2_400_000, 380_000)).toBe('−84%');
  });
  it('shows growth with a plus sign', () => {
    expect(savingPercent(100, 112)).toBe('+12%');
  });
  it('shows 0% for no change or an empty original', () => {
    expect(savingPercent(500, 500)).toBe('0%');
    expect(savingPercent(0, 10)).toBe('0%');
  });
  it('shows −100% when the result is empty', () => {
    expect(savingPercent(10, 0)).toBe('−100%');
  });
});
