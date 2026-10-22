import { describe, expect, it } from 'vitest';
import { getPath, withPath, type Settings } from './settings';
import { sample } from './settings.fixture';

describe('settings paths', () => {
  it('reads a nested value', () => {
    expect(getPath(sample, 'images.compression')).toBe(80);
    expect(getPath(sample, 'nope.nothing')).toBeUndefined();
  });

  it('changing one path keeps every other field and unknown ones', () => {
    const withUnknown = { ...sample, future: { flag: true } } as Settings;
    const next = withPath(withUnknown, 'images.compression', 55);
    expect(getPath(next, 'images.compression')).toBe(55);
    expect({ ...next, images: { ...next.images, compression: 80 } }).toEqual(withUnknown);
    expect(getPath(sample, 'images.compression')).toBe(80);
  });
});
