import { describe, expect, it } from 'vitest';
import { acceleratorFromEvent, hotkeyLabel, problemKey } from './hotkeys';

const none = { ctrlKey: false, altKey: false, shiftKey: false, metaKey: false };

describe('acceleratorFromEvent', () => {
  it('orders modifiers Ctrl, Alt, Shift, Cmd|Super', () => {
    expect(acceleratorFromEvent({ ...none, code: 'KeyC', ctrlKey: true, shiftKey: true }, true)).toBe(
      'Ctrl+Shift+C',
    );
    expect(acceleratorFromEvent({ ...none, code: 'KeyD', metaKey: true }, true)).toBe('Cmd+D');
    expect(acceleratorFromEvent({ ...none, code: 'KeyD', metaKey: true }, false)).toBe('Super+D');
    expect(acceleratorFromEvent({ ...none, code: 'ArrowUp', altKey: true }, true)).toBe(
      'Alt+ArrowUp',
    );
  });
  it('names digits, punctuation, function keys and space', () => {
    expect(acceleratorFromEvent({ ...none, code: 'Digit5' }, true)).toBe('5');
    expect(acceleratorFromEvent({ ...none, code: 'Minus' }, true)).toBe('Minus');
    expect(acceleratorFromEvent({ ...none, code: 'Space' }, true)).toBe('Space');
    expect(acceleratorFromEvent({ ...none, code: 'F5' }, true)).toBe('F5');
  });
  it('gives null for a lone modifier or a key Ferry cannot bind', () => {
    expect(acceleratorFromEvent({ ...none, code: 'ShiftLeft', shiftKey: true }, true)).toBeNull();
    expect(acceleratorFromEvent({ ...none, code: 'MetaRight', metaKey: true }, true)).toBeNull();
    expect(acceleratorFromEvent({ ...none, code: 'IntlBackslash' }, true)).toBeNull();
  });
});

describe('hotkeyLabel', () => {
  it('uses symbols on macOS in ⌃⌥⇧⌘ order', () => {
    expect(hotkeyLabel('Ctrl+Shift+C', true)).toBe('⌃⇧C');
    expect(hotkeyLabel('Shift+Ctrl+C', true)).toBe('⌃⇧C');
    expect(hotkeyLabel('Cmd+Alt+P', true)).toBe('⌥⌘P');
    expect(hotkeyLabel('CmdOrCtrl+D', true)).toBe('⌘D');
  });
  it('spells modifiers out on Windows', () => {
    expect(hotkeyLabel('CmdOrCtrl+D', false)).toBe('Ctrl+D');
    expect(hotkeyLabel('Ctrl+Alt+Shift+C', false)).toBe('Ctrl+Alt+Shift+C');
    expect(hotkeyLabel('Super+L', false)).toBe('Win+L');
  });
  it('shows bare keys by their name', () => {
    expect(hotkeyLabel('Minus', true)).toBe('-');
    expect(hotkeyLabel('-', false)).toBe('-');
    expect(hotkeyLabel('Space', true)).toBe('Space');
  });
});

describe('problemKey', () => {
  it('picks the message for the system', () => {
    expect(problemKey({ kind: 'needs_modifier' }, null, true)).toBe('svc.hotkey.needs_modifier.mac');
    expect(problemKey({ kind: 'reserved' }, null, false)).toBe('svc.hotkey.reserved.windows');
    expect(problemKey({ kind: 'duplicate', with: 'optimise' }, null, true)).toBe(
      'svc.hotkey.duplicate',
    );
  });
  it('warns about developer tools only without a problem', () => {
    expect(problemKey(null, 'dev_tools', false)).toBe('svc.hotkey.devtools');
    expect(problemKey(null, null, true)).toBeNull();
  });
});
