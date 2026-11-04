import { describe, expect, it } from 'vitest';
import {
  addExcludedApp,
  patchSection,
  removeExcludedApp,
  setActivationModifier,
  setGestureButton,
  setHotkey,
  setRules,
  setSnapModifier,
  withManagementState,
} from './settingsUpdate';
import { makeSettings } from '../test/fixtures';

describe('patchSection', () => {
  it('returns a new object and leaves other sections untouched', () => {
    const settings = makeSettings();
    const next = patchSection(settings, 'snapping', { distance: 24 });
    expect(next).not.toBe(settings);
    expect(next.snapping.distance).toBe(24);
    expect(settings.snapping.distance).toBe(16);
    expect(next.general).toBe(settings.general);
    expect(next.rules).toBe(settings.rules);
  });

  it('returns the same object when nothing changes', () => {
    const settings = makeSettings();
    expect(patchSection(settings, 'general', { theme: 'system' })).toBe(settings);
    expect(setRules(settings, settings.rules)).toBe(settings);
  });

  it('sets hotkeys per action', () => {
    const settings = makeSettings();
    const next = setHotkey(settings, 'opacityDown', null);
    expect(next.hotkeys.opacityDown).toBeNull();
    expect(next.hotkeys.togglePin).toBe(settings.hotkeys.togglePin);
  });
});

describe('gesture bindings', () => {
  it('moves the snap modifier away from a new activation modifier', () => {
    const settings = makeSettings();
    const next = setActivationModifier(settings, 'ctrl');
    expect(next.gestures.modifier).toBe('ctrl');
    expect(next.snapping.modifier).toBe('alt');

    const keep = setActivationModifier(settings, 'win');
    expect(keep.snapping.modifier).toBe('ctrl');
  });

  it('refuses a snap modifier equal to the activation modifier', () => {
    const settings = makeSettings();
    expect(setSnapModifier(settings, 'alt')).toBe(settings);
    expect(setSnapModifier(settings, 'shift').snapping.modifier).toBe('shift');
  });

  it('never assigns one mouse button to both gestures', () => {
    const settings = makeSettings();
    expect(setGestureButton(settings, 'moveButton', 'right')).toBe(settings);
    expect(setGestureButton(settings, 'moveButton', 'middle').gestures.moveButton).toBe('middle');
    expect(setGestureButton(settings, 'resizeButton', null).gestures.resizeButton).toBeNull();
  });
});

describe('excluded applications', () => {
  it('adds trimmed names and rejects duplicates and blanks', () => {
    const added = addExcludedApp(['Code.exe'], '  "blender.exe" ');
    expect(added).toEqual({ ok: true, list: ['Code.exe', 'blender.exe'] });
    const duplicate = addExcludedApp(['Code.exe'], 'code.EXE');
    expect(duplicate.ok).toBe(false);
    expect(addExcludedApp([], '   ').ok).toBe(false);
  });

  it('removes by index', () => {
    expect(removeExcludedApp(['a', 'b', 'c'], 1)).toEqual(['a', 'c']);
    expect(removeExcludedApp(['a'], 5)).toEqual(['a']);
  });
});

describe('withManagementState', () => {
  it('carries the runtime master switch into saved settings', () => {
    const settings = makeSettings();
    expect(withManagementState(settings, true)).toBe(settings);
    const paused = withManagementState(settings, false);
    expect(paused.general.managementEnabled).toBe(false);
    expect(settings.general.managementEnabled).toBe(true);
  });
});
