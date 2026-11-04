import type { Theme } from './types';

export type ResolvedTheme = 'light' | 'dark';

export function resolveTheme(theme: Theme, systemPrefersDark: boolean): ResolvedTheme {
  if (theme === 'system') return systemPrefersDark ? 'dark' : 'light';
  return theme;
}

export const THEME_LABELS: Record<Theme, string> = {
  system: 'Use system setting',
  light: 'Light',
  dark: 'Dark',
};
