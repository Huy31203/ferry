import type { StringKey } from './i18n';
import type { Problem } from './svc';

type KeyEvent = Pick<KeyboardEvent, 'code' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>;

const PASS_THROUGH = new Set([
  'Space',
  'Minus',
  'Equal',
  'Comma',
  'Period',
  'Slash',
  'Backslash',
  'Semicolon',
  'Quote',
  'BracketLeft',
  'BracketRight',
  'Backquote',
  'ArrowUp',
  'ArrowDown',
  'ArrowLeft',
  'ArrowRight',
  'Home',
  'End',
  'PageUp',
  'PageDown',
  'Insert',
  'Delete',
  'Backspace',
  'Enter',
  'Tab',
  'Escape',
]);

/** The key part of an accelerator for `event.code`, or null for a key Ferry can't bind. */
function keyToken(code: string): string | null {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1];
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return digit[1];
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code) || /^Numpad[0-9]$/.test(code)) return code;
  return PASS_THROUGH.has(code) ? code : null;
}

/** The accelerator for a keydown, or null for a lone modifier or a key Ferry can't bind. Order: Ctrl+Alt+Shift+Cmd|Super+Key. */
export function acceleratorFromEvent(e: KeyEvent, mac: boolean): string | null {
  const key = keyToken(e.code);
  if (key === null) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  if (e.metaKey) parts.push(mac ? 'Cmd' : 'Super');
  return [...parts, key].join('+');
}

const KEY_LABELS: Record<string, string> = {
  Minus: '-',
  Equal: '=',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
};

/** "⌃⇧C" on macOS (⌃⌥⇧⌘ order), "Ctrl+Alt+Shift+C" on Windows; CmdOrCtrl → ⌘ / Ctrl; Super → Win on Windows. */
export function hotkeyLabel(accelerator: string, mac: boolean): string {
  const mods = { ctrl: false, alt: false, shift: false, cmd: false };
  let key = '';
  for (const token of accelerator.split('+')) {
    switch (token.toLowerCase()) {
      case 'ctrl':
      case 'control':
        mods.ctrl = true;
        break;
      case 'alt':
      case 'option':
        mods.alt = true;
        break;
      case 'shift':
        mods.shift = true;
        break;
      case 'cmd':
      case 'command':
      case 'super':
        mods.cmd = true;
        break;
      case 'cmdorctrl':
      case 'commandorcontrol':
      case 'cmdorcontrol':
      case 'commandorctrl':
        if (mac) mods.cmd = true;
        else mods.ctrl = true;
        break;
      default:
        key = KEY_LABELS[token] ?? (token.length === 1 ? token.toUpperCase() : token);
    }
  }
  if (mac) {
    return (
      (mods.ctrl ? '⌃' : '') +
      (mods.alt ? '⌥' : '') +
      (mods.shift ? '⇧' : '') +
      (mods.cmd ? '⌘' : '') +
      key
    );
  }
  const parts: string[] = [];
  if (mods.ctrl) parts.push('Ctrl');
  if (mods.alt) parts.push('Alt');
  if (mods.shift) parts.push('Shift');
  if (mods.cmd) parts.push('Win');
  return [...parts, key].join('+');
}

/** The message key for a verdict or a failed status, or null. `duplicate`'s `{name}` is filled by the caller with the other binding's label. */
export function problemKey(
  problem: Problem | null,
  warning: 'dev_tools' | null,
  mac: boolean,
): StringKey | null {
  const system = mac ? 'mac' : 'windows';
  switch (problem?.kind) {
    case 'invalid':
      return 'svc.hotkey.invalid';
    case 'needs_modifier':
      return `svc.hotkey.needs_modifier.${system}`;
    case 'reserved':
      return `svc.hotkey.reserved.${system}`;
    case 'card_key':
      return 'svc.hotkey.card_key';
    case 'duplicate':
      return 'svc.hotkey.duplicate';
    default:
      return warning === 'dev_tools' ? 'svc.hotkey.devtools' : null;
  }
}
