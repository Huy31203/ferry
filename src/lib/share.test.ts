import { describe, expect, it } from 'vitest';
import { shareAnchor } from './share';

describe('shareAnchor', () => {
  it('copies the card box and points toward the screen centre', () => {
    const rect = { x: 12, y: 40, width: 300, height: 200 };
    expect(shareAnchor(rect, false)).toEqual({ x: 12, y: 40, width: 300, height: 200, toward: 'left' });
    expect(shareAnchor(rect, true).toward).toBe('right');
  });
});
