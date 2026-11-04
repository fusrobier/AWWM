import { describe, expect, it } from 'vitest';
import { buildCheatSheet, resolveSnapModifier, snapModifierOptions, summarizeGestures } from './gestures';
import { makeSettings } from '../test/fixtures';

describe('snap modifier', () => {
  it('never offers the activation modifier', () => {
    expect(snapModifierOptions('alt')).toEqual(['ctrl', 'shift', 'win']);
    expect(snapModifierOptions('ctrl')).not.toContain('ctrl');
  });

  it('resolves conflicts like the backend sanitizer', () => {
    expect(resolveSnapModifier('alt', 'ctrl')).toBe('ctrl');
    expect(resolveSnapModifier('ctrl', 'ctrl')).toBe('alt');
    expect(resolveSnapModifier('alt', 'alt')).toBe('ctrl');
    expect(resolveSnapModifier('win', 'win')).toBe('alt');
  });
});

describe('buildCheatSheet', () => {
  it('lists move, resize, snapping variants and Esc for the defaults', () => {
    const s = makeSettings();
    const sheet = buildCheatSheet(s.gestures, s.snapping, true);
    expect(sheet.active).toBe(true);
    expect(sheet.entries.map((e) => e.keys.join(' + '))).toEqual([
      'Alt + Left drag',
      'Alt + Ctrl + Left drag',
      'Alt + Right drag',
      'Alt + Ctrl + Right drag',
      'Esc',
    ]);
    expect(sheet.entries[1]?.action).toBe('Move with snapping');
  });

  it('follows the configured modifiers and buttons', () => {
    const s = makeSettings();
    const gestures = { ...s.gestures, modifier: 'win' as const, moveButton: 'middle' as const, resizeButton: null };
    const snapping = { ...s.snapping, enabled: false };
    const sheet = buildCheatSheet(gestures, snapping, true);
    expect(sheet.entries.map((e) => e.keys.join(' + '))).toEqual(['Win + Middle drag', 'Esc']);
  });

  it('explains why nothing is active', () => {
    const s = makeSettings();
    expect(buildCheatSheet({ ...s.gestures, enabled: false }, s.snapping, true)).toEqual({
      active: false,
      entries: [],
      notes: ['Mouse gestures are turned off.'],
    });
    const noButtons = buildCheatSheet({ ...s.gestures, moveButton: null, resizeButton: null }, s.snapping, true);
    expect(noButtons.active).toBe(false);
    expect(noButtons.entries).toEqual([]);
    const paused = buildCheatSheet(s.gestures, s.snapping, false);
    expect(paused.active).toBe(false);
    expect(paused.notes.some((n) => n.includes('paused'))).toBe(true);
  });

  it('mentions scope and exclusions', () => {
    const s = makeSettings();
    const sheet = buildCheatSheet({ ...s.gestures, scope: 'pinnedOnly', excludedApps: ['a.exe', 'b.exe'] }, s.snapping, true);
    expect(sheet.notes).toContain('Only pinned windows can be moved or resized.');
    expect(sheet.notes).toContain('Gestures are ignored in 2 excluded applications.');
  });
});

describe('summarizeGestures', () => {
  it('describes the bindings in one sentence', () => {
    const s = makeSettings();
    expect(summarizeGestures(s.gestures, s.snapping)).toBe(
      'Alt + Left drag moves a window, Alt + Right drag resizes it. Hold Ctrl as well to snap.',
    );
    expect(summarizeGestures({ ...s.gestures, enabled: false }, s.snapping)).toBe('Mouse gestures are turned off.');
  });
});
