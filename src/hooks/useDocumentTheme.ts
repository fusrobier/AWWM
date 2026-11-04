import { useEffect } from 'react';
import { resolveTheme, type ResolvedTheme } from '../lib/theme';
import type { Theme } from '../lib/types';

const DARK_QUERY = '(prefers-color-scheme: dark)';

export function applyDocumentTheme(resolved: ResolvedTheme): void {
  document.documentElement.dataset.theme = resolved;
}

/** Applies the system theme before React renders, so the first paint is correct. */
export function applyInitialTheme(): void {
  applyDocumentTheme(resolveTheme('system', window.matchMedia(DARK_QUERY).matches));
}

/** Applies `theme` to <html>; "system" follows the OS setting live. */
export function useDocumentTheme(theme: Theme): void {
  useEffect(() => {
    const media = window.matchMedia(DARK_QUERY);
    const apply = (): void => applyDocumentTheme(resolveTheme(theme, media.matches));
    apply();
    if (theme !== 'system') return undefined;
    media.addEventListener('change', apply);
    return () => media.removeEventListener('change', apply);
  }, [theme]);
}
