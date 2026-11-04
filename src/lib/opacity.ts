// Opacity rules shared by the dashboard, the rule editor and the pinning settings.
import { clamp } from './numbers';

/** Below this opacity (percent) a window is hard to find on screen. */
export const NEARLY_INVISIBLE_THRESHOLD = 10;

export const NEARLY_INVISIBLE_WARNING =
  'Window will be nearly invisible — use this list or the tray to restore it';

export const NEARLY_INVISIBLE_RULE_WARNING =
  'Matching windows will be nearly invisible. Use the dashboard or the tray menu to restore them.';

export function isNearlyInvisible(opacity: number): boolean {
  return opacity < NEARLY_INVISIBLE_THRESHOLD;
}

/** Rounds and clamps to the 0–100 range used by the backend. */
export function clampOpacity(value: number): number {
  return clamp(Math.round(value), 0, 100);
}
