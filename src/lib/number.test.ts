import { describe, expect, it } from 'vitest';
import { commitNumber } from './number';

describe('commitNumber', () => {
  it('clamps and rounds a typed number', () => {
    expect(commitNumber('40', false, 7, 1, 3650)).toBe(40);
    expect(commitNumber('0', false, 7, 1, 3650)).toBe(1);
    expect(commitNumber('99999', false, 7, 1, 3650)).toBe(3650);
    expect(commitNumber('2.6', false, 7, 1, 3650)).toBe(3);
  });

  it('never saves a blank or unreadable field as the minimum', () => {
    expect(commitNumber('', false, 7, 1, 3650)).toBeNull();
    expect(commitNumber('   ', false, 7, 1, 3650)).toBeNull();
    expect(commitNumber('', true, 7, 1, 3650)).toBeNull();
    expect(commitNumber('abc', false, 7, 1, 3650)).toBeNull();
  });

  it('saves nothing when the value did not change', () => {
    expect(commitNumber('7', false, 7, 1, 3650)).toBeNull();
  });
});
