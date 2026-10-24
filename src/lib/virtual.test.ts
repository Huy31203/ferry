import { describe, expect, it } from 'vitest';
import { visibleRange } from './virtual';

describe('visibleRange', () => {
  it('shows the rows in view plus the overscan and pads the rest', () => {
    expect(visibleRange(0, 320, 32, 5000)).toEqual({
      start: 0,
      end: 16,
      padTop: 0,
      padBottom: (5000 - 16) * 32,
    });
    expect(visibleRange(3200, 320, 32, 5000)).toEqual({
      start: 94,
      end: 116,
      padTop: 94 * 32,
      padBottom: (5000 - 116) * 32,
    });
    expect(visibleRange(0, 320, 32, 3)).toEqual({ start: 0, end: 3, padTop: 0, padBottom: 0 });
  });
});
