// Global keyboard shortcut helpers: labels, editing drafts, validation and conflicts.
import type { Hotkey, HotkeyAction, HotkeySettings, HotkeyStatus } from './types';

export const HOTKEY_ACTIONS: readonly HotkeyAction[] = ['togglePin', 'opacityUp', 'opacityDown'];

export interface HotkeyActionInfo {
  label: string;
  /** Completes "<shortcut> …" on the dashboard. */
  summary: string;
}

export const HOTKEY_ACTION_INFO: Record<HotkeyAction, HotkeyActionInfo> = {
  togglePin: { label: 'Pin or unpin the active window', summary: 'pins or unpins the active window' },
  opacityUp: { label: 'Increase opacity', summary: 'makes the active window more opaque' },
  opacityDown: { label: 'Decrease opacity', summary: 'makes the active window more transparent' },
};

export type HotkeyModifier = 'ctrl' | 'alt' | 'shift' | 'win';

/** Order of the modifier chips in the editor. */
export const HOTKEY_MODIFIERS: readonly HotkeyModifier[] = ['ctrl', 'alt', 'shift', 'win'];

export const HOTKEY_MODIFIER_LABELS: Record<HotkeyModifier, string> = {
  ctrl: 'Ctrl',
  alt: 'Alt',
  shift: 'Shift',
  win: 'Win',
};

/** Modifiers suggested when a disabled shortcut gets a key (the app's default scheme). */
export const DEFAULT_HOTKEY_MODIFIERS: Readonly<Record<HotkeyModifier, boolean>> = {
  ctrl: false,
  alt: true,
  shift: false,
  win: true,
};

/** `Win+Alt+T` — same order as the backend (Ctrl, Win, Alt, Shift, key). */
export function formatHotkey(hotkey: Hotkey): string {
  const parts: string[] = [];
  if (hotkey.ctrl) parts.push('Ctrl');
  if (hotkey.win) parts.push('Win');
  if (hotkey.alt) parts.push('Alt');
  if (hotkey.shift) parts.push('Shift');
  parts.push(hotkey.key);
  return parts.join('+');
}

const KEY_DISPLAY_NAMES: Record<string, string> = {
  PageUp: 'Page Up',
  PageDown: 'Page Down',
  ScrollLock: 'Scroll Lock',
  Up: 'Up arrow',
  Down: 'Down arrow',
  Left: 'Left arrow',
  Right: 'Right arrow',
  Plus: '= / +',
  Minus: '- / _',
  Comma: ', (comma)',
  Period: '. (period)',
  NumpadAdd: 'Numpad +',
  NumpadSubtract: 'Numpad −',
  NumpadMultiply: 'Numpad ×',
  NumpadDivide: 'Numpad ÷',
};

/** Friendlier name for the key picker (`PageUp` → `Page Up`, `Numpad7` → `Numpad 7`). */
export function keyDisplayName(key: string): string {
  const known = KEY_DISPLAY_NAMES[key];
  if (known !== undefined) return known;
  const numpad = /^Numpad(\d)$/.exec(key);
  if (numpad) return `Numpad ${numpad[1] ?? ''}`;
  return key;
}

/** Editable form of a shortcut: `key === null` means the shortcut is disabled. */
export interface HotkeyDraft {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  win: boolean;
  key: string | null;
}

/**
 * Builds a draft from a saved shortcut. For a disabled shortcut the modifiers of
 * `previous` are kept (so re-enabling restores them), falling back to Win+Alt.
 */
export function draftFromHotkey(hotkey: Hotkey | null, previous?: HotkeyDraft): HotkeyDraft {
  if (hotkey) {
    return { ctrl: hotkey.ctrl, alt: hotkey.alt, shift: hotkey.shift, win: hotkey.win, key: hotkey.key };
  }
  const base = previous && hasRequiredModifier(previous) ? previous : DEFAULT_HOTKEY_MODIFIERS;
  return { ctrl: base.ctrl, alt: base.alt, shift: base.shift, win: base.win, key: null };
}

export function hotkeyFromDraft(draft: HotkeyDraft): Hotkey | null {
  if (draft.key === null) return null;
  return { ctrl: draft.ctrl, alt: draft.alt, shift: draft.shift, win: draft.win, key: draft.key };
}

/**
 * A global shortcut needs Ctrl, Alt or Win. Shift alone is not enough: Shift+T would
 * swallow typing a capital T in every application (the backend rejects it too).
 */
export function hasRequiredModifier(value: Pick<HotkeyDraft, HotkeyModifier>): boolean {
  return value.ctrl || value.alt || value.win;
}

export function toggleModifier(draft: HotkeyDraft, modifier: HotkeyModifier): HotkeyDraft {
  return { ...draft, [modifier]: !draft[modifier] };
}

/** Canonical identity of a shortcut (key names are case-insensitive). */
export function hotkeySignature(hotkey: Hotkey): string {
  const flags = HOTKEY_MODIFIERS.map((m) => (hotkey[m] ? '1' : '0')).join('');
  return `${flags}:${hotkey.key.toLowerCase()}`;
}

export function hotkeysEqual(a: Hotkey | null, b: Hotkey | null): boolean {
  if (a === null || b === null) return a === b;
  return hotkeySignature(a) === hotkeySignature(b);
}

/** Error message for an invalid draft, or null. A disabled shortcut is always valid. */
export function validateHotkeyDraft(draft: HotkeyDraft, keyNames: readonly string[]): string | null {
  if (draft.key === null) return null;
  const key = draft.key.toLowerCase();
  if (keyNames.length > 0 && !keyNames.some((name) => name.toLowerCase() === key)) {
    return `${draft.key} is not a supported key.`;
  }
  if (!hasRequiredModifier(draft)) {
    return draft.shift
      ? 'Shift alone is not enough: add Ctrl, Alt or Win, or the shortcut would block normal typing.'
      : 'Choose at least one of Ctrl, Alt or Win.';
  }
  return null;
}

/** Another action that already uses `candidate`, if any. */
export function findConflictingAction(
  hotkeys: HotkeySettings,
  action: HotkeyAction,
  candidate: Hotkey | null,
): HotkeyAction | null {
  if (candidate === null) return null;
  for (const other of HOTKEY_ACTIONS) {
    if (other !== action && hotkeysEqual(hotkeys[other], candidate)) return other;
  }
  return null;
}

export type HotkeyPersistDecision = { save: true; hotkey: Hotkey | null } | { save: false };

/**
 * Decides whether an edited draft should be saved. Invalid drafts (no Ctrl/Alt/Win,
 * unknown key), combinations used by another action and unchanged shortcuts are never
 * persisted; they stay local in the editor.
 */
export function hotkeyToPersist(
  draft: HotkeyDraft,
  keyNames: readonly string[],
  hotkeys: HotkeySettings,
  action: HotkeyAction,
): HotkeyPersistDecision {
  if (validateHotkeyDraft(draft, keyNames) !== null) return { save: false };
  const hotkey = hotkeyFromDraft(draft);
  if (findConflictingAction(hotkeys, action, hotkey) !== null) return { save: false };
  if (hotkeysEqual(hotkey, hotkeys[action])) return { save: false };
  return { save: true, hotkey };
}

export type HotkeyStatusTone = 'ok' | 'error' | 'inactive';

export function describeHotkeyStatus(status: HotkeyStatus): { tone: HotkeyStatusTone; text: string } {
  if (status.registered) return { tone: 'ok', text: `${status.label} is active` };
  if (status.error) return { tone: 'error', text: `${status.label} is unavailable: ${status.error}` };
  return { tone: 'inactive', text: `${status.label} is not active` };
}
