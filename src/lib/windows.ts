// Helpers for managed windows and the window picker.
import type { ManagedSource, ManagedWindow, Rule } from './types';

/** Shown for windows of elevated processes (`WindowInfo.accessible === false`). */
export const ELEVATED_WINDOW_NOTE = 'Runs as administrator — Windows does not let AWWM change it';

/** Case-insensitive search over title and executable name; all terms must match. */
export function filterWindows<T extends { title: string; exeName: string }>(
  windows: readonly T[],
  query: string,
): T[] {
  const terms = query.trim().toLowerCase().split(/\s+/).filter((term) => term.length > 0);
  if (terms.length === 0) return [...windows];
  return windows.filter((window) => {
    const haystack = `${window.title}\n${window.exeName}`.toLowerCase();
    return terms.every((term) => haystack.includes(term));
  });
}

export function displayTitle(title: string): string {
  const trimmed = title.trim();
  return trimmed === '' ? 'Untitled window' : trimmed;
}

/** `notepad.exe` → `Notepad`, `CalculatorApp.exe` → `CalculatorApp`. */
export function friendlyAppName(exeName: string): string {
  const base = exeName.trim().replace(/^.*[\\/]/, '').replace(/\.exe$/i, '');
  if (base === '') return '';
  return base.charAt(0).toUpperCase() + base.slice(1);
}

export type SourceTone = 'neutral' | 'accent' | 'warning';

export function managedSourceTone(source: ManagedSource): SourceTone {
  switch (source) {
    case 'manual':
      return 'neutral';
    case 'rule':
      return 'accent';
    case 'recovered':
      return 'warning';
  }
}

/** `Manual`, `Rule: Notepad`, `Recovered`. */
export function managedSourceLabel(
  window: Pick<ManagedWindow, 'source' | 'ruleId'>,
  rules: readonly Rule[],
): string {
  switch (window.source) {
    case 'manual':
      return 'Manual';
    case 'recovered':
      return 'Recovered';
    case 'rule': {
      const rule = window.ruleId === null ? undefined : rules.find((r) => r.id === window.ruleId);
      if (!rule) return 'Rule (deleted)';
      const name = rule.name.trim() || rule.exe;
      return `Rule: ${name}`;
    }
  }
}

/** Replaces the entry with the same handle, or appends it. */
export function upsertManaged(list: readonly ManagedWindow[], updated: ManagedWindow): ManagedWindow[] {
  const index = list.findIndex((window) => window.hwnd === updated.hwnd);
  if (index === -1) return [...list, updated];
  return list.map((window, i) => (i === index ? updated : window));
}

export function removeManaged(list: readonly ManagedWindow[], hwnd: number): ManagedWindow[] {
  return list.filter((window) => window.hwnd !== hwnd);
}

/**
 * True when a command result describes a window AWWM no longer manages: neither pinned
 * nor translucent. The backend releases such windows (e.g. opacity raised back to 100 %
 * on an opacity-only window) and has already reported the removal.
 */
export function isReleased(window: Pick<ManagedWindow, 'pinned' | 'opacity'>): boolean {
  return !window.pinned && window.opacity >= 100;
}

/**
 * Applies a ManagedWindow returned by a command (pin, set opacity): a released window is
 * removed instead of being added back as a ghost row; anything else is upserted.
 */
export function applyManagedResult(list: readonly ManagedWindow[], updated: ManagedWindow): ManagedWindow[] {
  return isReleased(updated) ? removeManaged(list, updated.hwnd) : upsertManaged(list, updated);
}
