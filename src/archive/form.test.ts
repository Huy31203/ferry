import { describe, expect, it } from 'vitest';
import { checkForm, LEVELS } from './form';

const ok = { name: 'Archive.zip', password: '', again: '', split: '' };

describe('checkForm', () => {
  it('accepts a plain form', () => expect(checkForm(ok)).toBeNull());
  it('needs a name', () => expect(checkForm({ ...ok, name: '  ' })).toBe('archive.error.name'));
  it('rejects a slash in the name', () => expect(checkForm({ ...ok, name: 'a/b.zip' })).toBe('archive.error.name'));
  it('needs matching passwords', () => expect(checkForm({ ...ok, password: 'a', again: 'b' })).toBe('archive.mismatch'));
  it('levels follow 7-Zip', () => expect(LEVELS).toEqual([0, 1, 3, 5, 7, 9]));
});
