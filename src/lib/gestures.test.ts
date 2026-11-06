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
    const sheet = buildCheatSheet(s.gestures, s.snapping, s.alignment, true);
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
    const sheet = buildCheatSheet(gestures, snapping, s.alignment, true);
    expect(sheet.entries.map((e) => e.keys.join(' + '))).toEqual(['Win + Middle drag', 'Esc']);
  });

  it('explains why nothing is active', () => {
    const s = makeSettings();
    expect(buildCheatSheet({ ...s.gestures, enabled: false }, s.snapping, s.alignment, true)).toEqual({
      active: false,
      entries: [],
      notes: ['Mouse gestures are turned off.'],
    });
    const noButtons = buildCheatSheet(
      { ...s.gestures, moveButton: null, resizeButton: null },
      s.snapping,
      s.alignment,
      true,
    );
    expect(noButtons.active).toBe(false);
    expect(noButtons.entries).toEqual([]);
    const paused = buildCheatSheet(s.gestures, s.snapping, s.alignment, false);
    expect(paused.active).toBe(false);
    expect(paused.notes.some((n) => n.includes('paused'))).toBe(true);
  });

  it('mentions scope and exclusions', () => {
    const s = makeSettings();
    const sheet = buildCheatSheet(
      { ...s.gestures, scope: 'pinnedOnly', excludedApps: ['a.exe', 'b.exe'] },
      s.snapping,
      s.alignment,
      true,
    );
    expect(sheet.notes).toContain('Only pinned windows can be moved or resized.');
    expect(sheet.notes).toContain('Gestures are ignored in 2 excluded applications.');
  });

  it('mentions pixel orientation only when a move button is assigned', () => {
    const s = makeSettings();
    const note = 'While moving, the position is shown in pixels.';
    expect(buildCheatSheet(s.gestures, s.snapping, s.alignment, true).notes).toContain(note);
    const off = { ...s.alignment, pixelOrientation: false };
    expect(buildCheatSheet(s.gestures, s.snapping, off, true).notes).not.toContain(note);
    const resizeOnly = { ...s.gestures, moveButton: null };
    expect(buildCheatSheet(resizeOnly, s.snapping, s.alignment, true).notes).not.toContain(note);
  });

  it('mentions the visible grid with its size and the gestures that show it', () => {
    const s = makeSettings();
    const grid = { ...s.alignment, showGrid: true, gridSize: 24 };
    const notes = buildCheatSheet(s.gestures, s.snapping, grid, true).notes;
    expect(notes).toContain('A pixel grid (24 px) appears while you move or resize.');
    expect(notes).toContain('Holding Ctrl also snaps to the grid lines.');

    const moveOnly = buildCheatSheet({ ...s.gestures, resizeButton: null }, s.snapping, grid, true).notes;
    expect(moveOnly).toContain('A pixel grid (24 px) appears while you move.');
    const resizeOnly = buildCheatSheet({ ...s.gestures, moveButton: null }, s.snapping, grid, true).notes;
    expect(resizeOnly).toContain('A pixel grid (24 px) appears while you resize.');

    const hidden = buildCheatSheet(s.gestures, s.snapping, s.alignment, true).notes;
    expect(hidden.some((n) => n.includes('pixel grid'))).toBe(false);
  });

  it('mentions snapping to the grid only when snapping can use it', () => {
    const s = makeSettings();
    const grid = { ...s.alignment, showGrid: true };
    const isSnapNote = (n: string): boolean => n.includes('snaps to the grid');
    const noSnapping = { ...s.snapping, enabled: false };
    expect(buildCheatSheet(s.gestures, noSnapping, grid, true).notes.some(isSnapNote)).toBe(false);
    const noSnapToGrid = { ...grid, snapToGrid: false };
    expect(buildCheatSheet(s.gestures, s.snapping, noSnapToGrid, true).notes.some(isSnapNote)).toBe(false);
    const shift = { ...s.snapping, modifier: 'shift' as const };
    expect(buildCheatSheet(s.gestures, shift, grid, true).notes).toContain('Holding Shift also snaps to the grid lines.');
  });

  it('adds no alignment notes when no gesture can show them', () => {
    const s = makeSettings();
    const all = { ...s.alignment, pixelOrientation: true, showGrid: true };
    const none = buildCheatSheet({ ...s.gestures, moveButton: null, resizeButton: null }, s.snapping, all, true);
    expect(none.notes).toEqual(['No mouse button is assigned. Choose a move or resize button.']);
    expect(buildCheatSheet({ ...s.gestures, enabled: false }, s.snapping, all, true).notes).toEqual([
      'Mouse gestures are turned off.',
    ]);
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
