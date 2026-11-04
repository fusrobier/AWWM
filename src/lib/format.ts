// Small, locale-neutral text formatting helpers shared by the pages.

/** Non-breaking space, used between a number and its unit so they never wrap apart. */
export const NBSP = ' ';

/** `1 window`, `3 windows`. */
export function pluralize(count: number, singular: string, plural = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : plural}`;
}

/** `85 %` (Windows style, with a non-breaking space). */
export function formatPercent(value: number): string {
  return `${Math.round(value)}${NBSP}%`;
}

/** `16 px`. */
export function formatPixels(value: number): string {
  return `${Math.round(value)}${NBSP}px`;
}

/** Joins truthy class names. */
export function cx(...parts: ReadonlyArray<string | false | null | undefined>): string {
  return parts.filter((part): part is string => typeof part === 'string' && part.length > 0).join(' ');
}

/** `#RRGGBB` (the only colour format the backend accepts). */
export function isHexColor(value: string): boolean {
  return /^#[0-9a-f]{6}$/i.test(value);
}
