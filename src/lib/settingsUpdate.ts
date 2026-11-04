// Immutable update helpers for Settings. Every helper returns the *same* object when
// nothing changes, so callers can skip a save by comparing identities.
import type {
  Hotkey,
  HotkeyAction,
  Modifier,
  MouseButton,
  Rule,
  Settings,
} from './types';
import { resolveSnapModifier } from './gestures';
import { normalizeExePattern } from './rules';

export type SettingsSection = 'general' | 'pinning' | 'gestures' | 'snapping' | 'hotkeys';

/** Shallow-merges `patch` into one settings section. */
export function patchSection<K extends SettingsSection>(
  settings: Settings,
  section: K,
  patch: Partial<Settings[K]>,
): Settings {
  const current = settings[section];
  const keys = Object.keys(patch) as Array<keyof Settings[K]>;
  const changed = keys.some((key) => !Object.is(current[key], patch[key]));
  if (!changed) return settings;
  const next: Settings = { ...settings };
  next[section] = { ...current, ...patch };
  return next;
}

export function setRules(settings: Settings, rules: Rule[]): Settings {
  return rules === settings.rules ? settings : { ...settings, rules };
}

export function setHotkey(settings: Settings, action: HotkeyAction, hotkey: Hotkey | null): Settings {
  switch (action) {
    case 'togglePin':
      return patchSection(settings, 'hotkeys', { togglePin: hotkey });
    case 'opacityUp':
      return patchSection(settings, 'hotkeys', { opacityUp: hotkey });
    case 'opacityDown':
      return patchSection(settings, 'hotkeys', { opacityDown: hotkey });
  }
}

/**
 * Changes the gesture activation modifier. The snap modifier must differ from it, so a
 * conflicting snap modifier is replaced the same way the backend's sanitizer does.
 */
export function setActivationModifier(settings: Settings, modifier: Modifier): Settings {
  const withModifier = patchSection(settings, 'gestures', { modifier });
  return patchSection(withModifier, 'snapping', {
    modifier: resolveSnapModifier(modifier, withModifier.snapping.modifier),
  });
}

/** Sets the snap modifier unless it equals the activation modifier. */
export function setSnapModifier(settings: Settings, modifier: Modifier): Settings {
  if (modifier === settings.gestures.modifier) return settings;
  return patchSection(settings, 'snapping', { modifier });
}

export type GestureButtonSlot = 'moveButton' | 'resizeButton';

/** Assigns a mouse button to move or resize; refuses to use one button for both. */
export function setGestureButton(
  settings: Settings,
  slot: GestureButtonSlot,
  button: MouseButton | null,
): Settings {
  const other = slot === 'moveButton' ? settings.gestures.resizeButton : settings.gestures.moveButton;
  if (button !== null && button === other) return settings;
  return slot === 'moveButton'
    ? patchSection(settings, 'gestures', { moveButton: button })
    : patchSection(settings, 'gestures', { resizeButton: button });
}

export type ListEditResult = { ok: true; list: string[] } | { ok: false; error: string };

/** Maximum number of excluded applications accepted by the backend. */
export const MAX_EXCLUSIONS = 500;

/** Adds an executable name/pattern to the gesture exclusion list. */
export function addExcludedApp(list: readonly string[], raw: string): ListEditResult {
  const name = normalizeExePattern(raw);
  if (name === '') return { ok: false, error: 'Enter an executable name, for example blender.exe.' };
  const lower = name.toLowerCase();
  if (list.some((item) => item.toLowerCase() === lower)) {
    return { ok: false, error: `${name} is already excluded.` };
  }
  if (list.length >= MAX_EXCLUSIONS) {
    return { ok: false, error: `At most ${MAX_EXCLUSIONS} applications can be excluded.` };
  }
  return { ok: true, list: [...list, name] };
}

export function removeExcludedApp(list: readonly string[], index: number): string[] {
  if (index < 0 || index >= list.length) return [...list];
  return list.filter((_, i) => i !== index);
}

/**
 * The master switch is changed through its own command (`setManagementEnabled`), and the
 * tray can toggle it too. Settings edited in the UI must never overwrite that runtime
 * state with a stale value, so the latest known value is carried into every save.
 */
export function withManagementState(settings: Settings, managementEnabled: boolean): Settings {
  return patchSection(settings, 'general', { managementEnabled });
}
