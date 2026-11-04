import { describe, expect, it } from 'vitest';
import {
  describeHotkeyStatus,
  draftFromHotkey,
  findConflictingAction,
  formatHotkey,
  hotkeyFromDraft,
  hotkeyToPersist,
  hotkeysEqual,
  keyDisplayName,
  toggleModifier,
  validateHotkeyDraft,
  type HotkeyDraft,
} from './hotkeys';
import type { Hotkey } from './types';
import { makeSettings } from '../test/fixtures';

const winAltT: Hotkey = { ctrl: false, alt: true, shift: false, win: true, key: 'T' };
const KEYS = ['T', 'PageUp', 'PageDown', 'F9'];

describe('formatHotkey', () => {
  it('uses the backend order Ctrl, Win, Alt, Shift', () => {
    expect(formatHotkey(winAltT)).toBe('Win+Alt+T');
    expect(formatHotkey({ ctrl: true, alt: true, shift: true, win: true, key: 'F9' })).toBe(
      'Ctrl+Win+Alt+Shift+F9',
    );
  });

  it('shows friendly key names in the picker', () => {
    expect(keyDisplayName('PageUp')).toBe('Page Up');
    expect(keyDisplayName('Numpad7')).toBe('Numpad 7');
    expect(keyDisplayName('F5')).toBe('F5');
  });
});

describe('drafts', () => {
  it('round-trips a hotkey through a draft', () => {
    expect(hotkeyFromDraft(draftFromHotkey(winAltT))).toEqual(winAltT);
  });

  it('suggests Win+Alt (or the previous modifiers) for a disabled shortcut', () => {
    expect(draftFromHotkey(null)).toEqual({ ctrl: false, alt: true, shift: false, win: true, key: null });
    const previous: HotkeyDraft = { ctrl: true, alt: false, shift: true, win: false, key: 'T' };
    expect(draftFromHotkey(null, previous)).toEqual({ ...previous, key: null });
    expect(hotkeyFromDraft(draftFromHotkey(null))).toBeNull();
  });

  it('toggles modifiers immutably', () => {
    const draft = draftFromHotkey(winAltT);
    const toggled = toggleModifier(draft, 'ctrl');
    expect(toggled.ctrl).toBe(true);
    expect(draft.ctrl).toBe(false);
  });
});

describe('validation', () => {
  it('requires Ctrl, Alt or Win', () => {
    const draft: HotkeyDraft = { ctrl: false, alt: false, shift: false, win: false, key: 'T' };
    expect(validateHotkeyDraft(draft, KEYS)).toContain('Ctrl, Alt or Win');
    expect(validateHotkeyDraft({ ...draft, ctrl: true }, KEYS)).toBeNull();
    expect(validateHotkeyDraft({ ...draft, alt: true }, KEYS)).toBeNull();
    expect(validateHotkeyDraft({ ...draft, win: true }, KEYS)).toBeNull();
  });

  it('rejects Shift as the only modifier, but allows it in combination', () => {
    const shiftOnly: HotkeyDraft = { ctrl: false, alt: false, shift: true, win: false, key: 'T' };
    expect(validateHotkeyDraft(shiftOnly, KEYS)).toContain('Shift alone');
    expect(validateHotkeyDraft({ ...shiftOnly, key: 'F9' }, KEYS)).toContain('Shift alone');
    expect(validateHotkeyDraft({ ...shiftOnly, ctrl: true }, KEYS)).toBeNull();
    expect(validateHotkeyDraft({ ...shiftOnly, win: true }, KEYS)).toBeNull();
  });

  it('accepts a disabled shortcut and rejects unknown keys', () => {
    expect(validateHotkeyDraft({ ctrl: false, alt: false, shift: false, win: false, key: null }, KEYS)).toBeNull();
    expect(validateHotkeyDraft({ ...draftFromHotkey(winAltT), key: 'Nope' }, KEYS)).toContain('not a supported key');
    expect(validateHotkeyDraft({ ...draftFromHotkey(winAltT), key: 'pageup' }, KEYS)).toBeNull();
  });

  it('does not reuse Shift-only modifiers when re-enabling a shortcut', () => {
    const previous: HotkeyDraft = { ctrl: false, alt: false, shift: true, win: false, key: 'T' };
    expect(draftFromHotkey(null, previous)).toEqual({ ctrl: false, alt: true, shift: false, win: true, key: null });
  });

  it('compares hotkeys case-insensitively and detects conflicts', () => {
    expect(hotkeysEqual(winAltT, { ...winAltT, key: 't' })).toBe(true);
    expect(hotkeysEqual(winAltT, null)).toBe(false);
    expect(hotkeysEqual(null, null)).toBe(true);

    const { hotkeys } = makeSettings();
    expect(findConflictingAction(hotkeys, 'opacityUp', winAltT)).toBe('togglePin');
    expect(findConflictingAction(hotkeys, 'togglePin', winAltT)).toBeNull();
    expect(findConflictingAction(hotkeys, 'opacityUp', null)).toBeNull();
  });
});

describe('hotkeyToPersist', () => {
  const { hotkeys } = makeSettings();
  const draftOf = (hotkey: Hotkey): HotkeyDraft => draftFromHotkey(hotkey);

  it('never persists invalid intermediate combinations', () => {
    // Removing Win and Alt from Win+Alt+T while heading for Ctrl+Shift+T passes Shift+T.
    const shiftOnly: HotkeyDraft = { ctrl: false, alt: false, shift: true, win: false, key: 'T' };
    expect(hotkeyToPersist(shiftOnly, KEYS, hotkeys, 'togglePin')).toEqual({ save: false });
    const none: HotkeyDraft = { ...shiftOnly, shift: false };
    expect(hotkeyToPersist(none, KEYS, hotkeys, 'togglePin')).toEqual({ save: false });
    expect(hotkeyToPersist({ ...shiftOnly, key: 'Nope', ctrl: true }, KEYS, hotkeys, 'togglePin')).toEqual({
      save: false,
    });
  });

  it('skips conflicts and unchanged shortcuts, saves valid changes and disabling', () => {
    const pageUp = hotkeys.opacityUp;
    if (!pageUp) throw new Error('fixture has opacityUp');
    expect(hotkeyToPersist(draftOf(pageUp), KEYS, hotkeys, 'togglePin')).toEqual({ save: false });
    expect(hotkeyToPersist(draftOf(winAltT), KEYS, hotkeys, 'togglePin')).toEqual({ save: false });

    const ctrlShiftT: Hotkey = { ctrl: true, alt: false, shift: true, win: false, key: 'T' };
    expect(hotkeyToPersist(draftOf(ctrlShiftT), KEYS, hotkeys, 'togglePin')).toEqual({ save: true, hotkey: ctrlShiftT });
    expect(hotkeyToPersist({ ...draftOf(winAltT), key: null }, KEYS, hotkeys, 'togglePin')).toEqual({
      save: true,
      hotkey: null,
    });
  });
});

describe('describeHotkeyStatus', () => {
  it('reports registered, failed and inactive shortcuts', () => {
    expect(describeHotkeyStatus({ action: 'togglePin', label: 'Win+Alt+T', registered: true, error: null }).tone).toBe('ok');
    const failed = describeHotkeyStatus({
      action: 'togglePin',
      label: 'Win+Alt+T',
      registered: false,
      error: 'Already used by another application',
    });
    expect(failed).toEqual({
      tone: 'error',
      text: 'Win+Alt+T is unavailable: Already used by another application',
    });
    expect(describeHotkeyStatus({ action: 'opacityUp', label: 'Win+Alt+PageUp', registered: false, error: null }).tone).toBe(
      'inactive',
    );
  });
});
