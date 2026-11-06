// Mouse-gesture vocabulary and the live "cheat sheet" derived from the settings.
import type { AlignmentSettings, GestureSettings, Modifier, MouseButton, SnapSettings } from './types';
import { formatPixels, pluralize } from './format';

/** Same order as the backend's `Modifier::ALL`. */
export const MODIFIERS: readonly Modifier[] = ['alt', 'ctrl', 'shift', 'win'];

export const MODIFIER_LABELS: Record<Modifier, string> = {
  alt: 'Alt',
  ctrl: 'Ctrl',
  shift: 'Shift',
  win: 'Win',
};

export const MOUSE_BUTTONS: readonly MouseButton[] = ['left', 'right', 'middle'];

export const BUTTON_LABELS: Record<MouseButton, string> = {
  left: 'Left',
  right: 'Right',
  middle: 'Middle',
};

/** `Left drag`. */
export function dragLabel(button: MouseButton): string {
  return `${BUTTON_LABELS[button]} drag`;
}

/** Snap modifiers the user may choose: anything except the activation modifier. */
export function snapModifierOptions(activation: Modifier): Modifier[] {
  return MODIFIERS.filter((modifier) => modifier !== activation);
}

/**
 * Returns `current` unless it collides with the activation modifier; then picks the
 * replacement the backend sanitizer would pick (first of Alt/Ctrl/Shift that differs).
 */
export function resolveSnapModifier(activation: Modifier, current: Modifier): Modifier {
  if (current !== activation) return current;
  return MODIFIERS.find((modifier) => modifier !== activation && modifier !== 'win') ?? 'ctrl';
}

export interface CheatSheetEntry {
  /** Key caps, e.g. `['Alt', 'Ctrl', 'Left drag']`. */
  keys: string[];
  action: string;
}

export interface CheatSheet {
  /** False when nothing can currently be triggered with the mouse. */
  active: boolean;
  entries: CheatSheetEntry[];
  notes: string[];
}

/** Notes about pixel orientation and the visible pixel grid, for the gestures that exist. */
function alignmentNotes(gestures: GestureSettings, snapping: SnapSettings, alignment: AlignmentSettings): string[] {
  const notes: string[] = [];
  if (alignment.pixelOrientation && gestures.moveButton) {
    notes.push('While moving, the position is shown in pixels.');
  }
  if (alignment.showGrid && (gestures.moveButton || gestures.resizeButton)) {
    const gesture =
      gestures.moveButton && gestures.resizeButton ? 'move or resize' : gestures.moveButton ? 'move' : 'resize';
    notes.push(`A pixel grid (${formatPixels(alignment.gridSize)}) appears while you ${gesture}.`);
    if (snapping.enabled && alignment.snapToGrid) {
      notes.push(`Holding ${MODIFIER_LABELS[snapping.modifier]} also snaps to the grid lines.`);
    }
  }
  return notes;
}

export function buildCheatSheet(
  gestures: GestureSettings,
  snapping: SnapSettings,
  alignment: AlignmentSettings,
  managementEnabled: boolean,
): CheatSheet {
  if (!gestures.enabled) {
    return { active: false, entries: [], notes: ['Mouse gestures are turned off.'] };
  }

  const modifier = MODIFIER_LABELS[gestures.modifier];
  const snapModifier = MODIFIER_LABELS[snapping.modifier];
  const entries: CheatSheetEntry[] = [];
  const notes: string[] = [];

  if (gestures.moveButton) {
    const drag = dragLabel(gestures.moveButton);
    entries.push({ keys: [modifier, drag], action: 'Move the window' });
    if (snapping.enabled) {
      entries.push({ keys: [modifier, snapModifier, drag], action: 'Move with snapping' });
    }
  }
  if (gestures.resizeButton) {
    const drag = dragLabel(gestures.resizeButton);
    entries.push({ keys: [modifier, drag], action: 'Resize from the nearest edge or corner' });
    if (snapping.enabled) {
      entries.push({ keys: [modifier, snapModifier, drag], action: 'Resize with snapping' });
    }
  }

  if (entries.length === 0) {
    notes.push('No mouse button is assigned. Choose a move or resize button.');
  } else {
    entries.push({ keys: ['Esc'], action: 'Cancel and restore the original position and size' });
    if (snapping.enabled) {
      notes.push(`Press or release ${snapModifier} during a drag to switch snapping on or off.`);
    }
    notes.push(...alignmentNotes(gestures, snapping, alignment));
  }

  if (gestures.scope === 'pinnedOnly') notes.push('Only pinned windows can be moved or resized.');
  if (gestures.excludedApps.length > 0) {
    notes.push(`Gestures are ignored in ${pluralize(gestures.excludedApps.length, 'excluded application')}.`);
  }
  if (!managementEnabled) {
    notes.push('Window management is paused, so gestures are inactive until it is turned back on.');
  }

  return { active: managementEnabled && entries.length > 1, entries, notes };
}

/** One-sentence summary for the dashboard. */
export function summarizeGestures(gestures: GestureSettings, snapping: SnapSettings): string {
  if (!gestures.enabled) return 'Mouse gestures are turned off.';
  const modifier = MODIFIER_LABELS[gestures.modifier];
  const parts: string[] = [];
  if (gestures.moveButton) parts.push(`${modifier} + ${dragLabel(gestures.moveButton)} moves a window`);
  if (gestures.resizeButton) parts.push(`${modifier} + ${dragLabel(gestures.resizeButton)} resizes it`);
  if (parts.length === 0) return 'Mouse gestures are on, but no mouse button is assigned.';
  const sentence = parts.join(', ');
  const scope = gestures.scope === 'pinnedOnly' ? ' (pinned windows only)' : '';
  const snap = snapping.enabled ? ` Hold ${MODIFIER_LABELS[snapping.modifier]} as well to snap.` : '';
  return `${sentence}${scope}.${snap}`;
}
