import { describe, expect, it } from 'vitest';
import { matches } from './fuzzy';

describe('matches', () => {
  it('needs every word somewhere in the fields, in any case', () => {
    expect(matches('LAUNCH login', 'Launch at login', 'Start Ferry when you sign in.', 'General')).toBe(true);
    expect(matches('launch dock', 'Launch at login', 'Start Ferry', 'General')).toBe(false);
  });

  it('looks in the help text and the pane name too', () => {
    expect(matches('sign', 'Launch at login', 'Start Ferry when you sign in.', 'General')).toBe(true);
    expect(matches('general', 'Launch at login', '', 'General')).toBe(true);
  });

  it('matches everything for a blank query', () => {
    expect(matches('   ', 'Anything')).toBe(true);
  });
});
