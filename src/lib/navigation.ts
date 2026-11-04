import type { Page } from './types';

/** Sidebar order. */
export const PAGES: readonly Page[] = ['dashboard', 'rules', 'mouse', 'general', 'about'];

export const PAGE_TITLES: Record<Page, string> = {
  dashboard: 'Dashboard',
  rules: 'Automatic rules',
  mouse: 'Mouse controls',
  general: 'General',
  about: 'About',
};

export function isPage(value: unknown): value is Page {
  return typeof value === 'string' && (PAGES as readonly string[]).includes(value);
}

/**
 * The page requested by the URL fragment. The backend opens the settings window at
 * `index.html#<page>` (e.g. `#dashboard` from the tray); anything else yields null.
 */
export function pageFromHash(hash: string): Page | null {
  const value = hash.startsWith('#') ? hash.slice(1) : hash;
  return isPage(value) ? value : null;
}

/** Initial page for a freshly opened window: the URL fragment, else the dashboard. */
export function initialPage(hash: string): Page {
  return pageFromHash(hash) ?? 'dashboard';
}

/** Index of the next item for arrow-key navigation in a vertical list (wraps around). */
export function nextIndex(current: number, count: number, key: string): number | null {
  if (count <= 0) return null;
  switch (key) {
    case 'ArrowDown':
      return (current + 1) % count;
    case 'ArrowUp':
      return (current - 1 + count) % count;
    case 'Home':
      return 0;
    case 'End':
      return count - 1;
    default:
      return null;
  }
}
