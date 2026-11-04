// Numeric helpers for clamped integer settings (snap distance, grid size, grace period …).

export function clamp(value: number, min: number, max: number): number {
  if (Number.isNaN(value)) return min;
  return Math.min(max, Math.max(min, value));
}

export type IntegerInput =
  | { kind: 'valid'; value: number }
  | { kind: 'outOfRange'; value: number; clamped: number }
  | { kind: 'empty' }
  | { kind: 'invalid' };

/**
 * Interprets the text of an integer field. Only plain (optionally signed) decimal
 * integers are accepted; values outside `[min, max]` report the clamped value so the
 * field can be corrected when it loses focus.
 */
export function interpretIntegerInput(text: string, min: number, max: number): IntegerInput {
  const trimmed = text.trim();
  if (trimmed === '') return { kind: 'empty' };
  if (!/^[-+]?\d+$/.test(trimmed)) return { kind: 'invalid' };
  const value = Number.parseInt(trimmed, 10);
  if (!Number.isFinite(value)) return { kind: 'invalid' };
  if (value < min || value > max) {
    return { kind: 'outOfRange', value, clamped: clamp(value, min, max) };
  }
  return { kind: 'valid', value };
}
