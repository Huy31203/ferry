import { describe, expect, it } from 'vitest';
import en from './i18n/en.json';
import { t } from './i18n';

describe('t', () => {
  it('returns the English string', () => {
    expect(t('tray.quit')).toBe('Quit Ferry');
  });
  it('fills placeholders', () => {
    expect(t('card.aria', { name: 'shot.png', state: 'optimised' })).toBe('shot.png, optimised');
  });
  it('leaves unknown placeholders visible', () => {
    expect(t('card.aria', { name: 'shot.png' })).toBe('shot.png, {state}');
  });
});

describe('en.json', () => {
  it('is a flat map of strings, because Rust parses it too', () => {
    for (const value of Object.values(en)) {
      expect(typeof value).toBe('string');
    }
  });
});
