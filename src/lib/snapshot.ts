import type { AppSnapshot } from './types';

/**
 * Merges a snapshot that arrives with a settings change (a `saveSettings` /
 * `resetSettings` result or an `awwm://snapshot` event) into the current state.
 *
 * The managed-window list in such snapshots can be older than the list the UI already
 * has: `awwm://managed-changed` is the authoritative stream for it (plus explicit
 * `getSnapshot()` loads), so the current list is kept. Before the first load there is
 * nothing to keep and the incoming snapshot is used as is.
 */
export function mergeSnapshot(current: AppSnapshot | null, incoming: AppSnapshot): AppSnapshot {
  if (current === null) return incoming;
  return { ...incoming, managed: current.managed };
}
