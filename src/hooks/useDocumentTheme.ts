import { useEffect, useSyncExternalStore } from 'react';
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

function subscribeToAppliedTheme(onChange: () => void): () => void {
  const observer = new MutationObserver(onChange);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
  return () => observer.disconnect();
}

const getAppliedTheme = (): ResolvedTheme =>
  document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';

/**
 * The theme currently applied to <html>. Changes only after the new theme tokens are in
 * effect, so components that read CSS custom properties (e.g. to paint a canvas) can
 * repaint when it changes.
 */
export function useAppliedTheme(): ResolvedTheme {
  return useSyncExternalStore(subscribeToAppliedTheme, getAppliedTheme, () => 'light');
}
