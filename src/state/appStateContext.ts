import { createContext, useContext } from 'react';
import type { SaveStatus } from '../lib/saveQueue';
import type { AppSnapshot, ManagedWindow, Page, Settings } from '../lib/types';

export type SettingsUpdater = (current: Settings) => Settings;

export interface AppState {
  snapshot: AppSnapshot;
  /** Settings as shown in the UI: local edits that are not saved yet, else the snapshot's. */
  settings: Settings;
  saveStatus: SaveStatus;
  /** Message of the last failed save, cleared by the next successful one. */
  saveError: string | null;
  /** Debounced save, for text and number fields. */
  editSettings: (updater: SettingsUpdater) => void;
  /** Immediate save, for toggles, selects, slider commits and dialogs. Resolves to success. */
  commitSettings: (updater: SettingsUpdater) => Promise<boolean>;
  /** Saves pending debounced edits now and waits for all saves to finish. */
  flushSettings: () => Promise<void>;
  updateManaged: (update: (managed: ManagedWindow[]) => ManagedWindow[]) => void;
  setManagementEnabled: (enabled: boolean) => Promise<boolean>;
  resetSettings: () => Promise<boolean>;
  /** The settings warning the user dismissed (so a new warning shows again). */
  dismissedWarning: string | null;
  dismissWarning: () => void;
}

export const AppStateContext = createContext<AppState | null>(null);

export function useAppState(): AppState {
  const context = useContext(AppStateContext);
  if (!context) throw new Error('useAppState() must be used inside <AppStateProvider>.');
  return context;
}

export interface Navigation {
  page: Page;
  navigate: (page: Page) => void;
}

export const NavigationContext = createContext<Navigation | null>(null);

export function useNavigation(): Navigation {
  const context = useContext(NavigationContext);
  if (!context) throw new Error('useNavigation() must be used inside <NavigationContext>.');
  return context;
}
