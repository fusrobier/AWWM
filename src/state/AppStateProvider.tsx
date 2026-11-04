// Owns the application snapshot, the settings save pipeline and the backend event
// subscriptions. Pages read and change state only through useAppState().
import {
  useCallback,
  useEffect,
  useEffectEvent,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { api, events } from '../lib/api';
import {
  BACKEND_UNAVAILABLE_MESSAGE,
  BACKEND_UNAVAILABLE_TITLE,
  errorMessage,
  isBackendAvailable,
} from '../lib/backend';
import { isPage } from '../lib/navigation';
import { SaveQueue, type SaveStatus } from '../lib/saveQueue';
import { mergeSnapshot } from '../lib/snapshot';
import { patchSection, withManagementState } from '../lib/settingsUpdate';
import type { AppSnapshot, ManagedWindow, Notice, Page, Settings } from '../lib/types';
import { StatusScreen } from '../components/StatusScreen';
import { useDocumentTheme } from '../hooks/useDocumentTheme';
import { AppStateContext, type AppState, type SettingsUpdater } from './appStateContext';
import { useToasts } from './toastContext';

/** Delay before text and number edits are saved. */
const SAVE_DEBOUNCE_MS = 400;

type LoadState = { status: 'loading' } | { status: 'error'; error: unknown } | { status: 'ready' };

function release(unlisten: UnlistenFn): void {
  try {
    const result: unknown = unlisten();
    if (result instanceof Promise) {
      result.catch((error: unknown) => console.warn('AWWM: could not remove an event listener', error));
    }
  } catch (error) {
    console.warn('AWWM: could not remove an event listener', error);
  }
}

interface AppStateProviderProps {
  children: ReactNode;
  /** Called when the tray asks for a page. */
  onNavigate: (page: Page) => void;
}

export function AppStateProvider({ children, onNavigate }: AppStateProviderProps) {
  const { notify, notifyError } = useToasts();
  const [load, setLoad] = useState<LoadState>({ status: 'loading' });
  const [snapshot, setSnapshotState] = useState<AppSnapshot | null>(null);
  const [draft, setDraftState] = useState<Settings | null>(null);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>('idle');
  const [saveError, setSaveError] = useState<string | null>(null);
  const [dismissedWarning, setDismissedWarning] = useState<string | null>(null);

  // Refs mirror the latest state so callbacks compose several updates within one tick.
  const snapshotRef = useRef<AppSnapshot | null>(null);
  const draftRef = useRef<Settings | null>(null);

  const setSnapshot = useCallback((next: AppSnapshot) => {
    snapshotRef.current = next;
    setSnapshotState(next);
  }, []);

  const updateSnapshot = useCallback(
    (update: (current: AppSnapshot) => AppSnapshot) => {
      const current = snapshotRef.current;
      if (current) setSnapshot(update(current));
    },
    [setSnapshot],
  );

  const setDraft = useCallback((next: Settings | null) => {
    draftRef.current = next;
    setDraftState(next);
  }, []);

  const [queue] = useState(
    () =>
      new SaveQueue<Settings, AppSnapshot>({
        delayMs: SAVE_DEBOUNCE_MS,
        save: (settings) =>
          api.saveSettings(
            withManagementState(
              settings,
              snapshotRef.current?.managementEnabled ?? settings.general.managementEnabled,
            ),
          ),
        onSuccess: (result, saved, superseded) => {
          // Keep the managed-window list: awwm://managed-changed is authoritative for it.
          setSnapshot(mergeSnapshot(snapshotRef.current, result));
          setSaveError(null);
          // The sanitized result becomes the source of truth unless newer edits are waiting.
          if (!superseded && draftRef.current === saved) setDraft(null);
        },
        onFailure: (error, saved, superseded) => {
          setSaveError(errorMessage(error));
          notifyError(error, 'Settings were not saved');
          // Show what is actually stored again, unless newer edits are on their way.
          if (!superseded && draftRef.current === saved) setDraft(null);
        },
        onStatusChange: setSaveStatus,
      }),
  );

  const fetchSnapshot = useCallback(async () => {
    try {
      setSnapshot(await api.getSnapshot());
      setLoad({ status: 'ready' });
    } catch (error) {
      setLoad({ status: 'error', error });
    }
  }, [setSnapshot]);

  const retry = useCallback(() => {
    setLoad({ status: 'loading' });
    void fetchSnapshot();
  }, [fetchSnapshot]);

  useEffect(() => {
    void fetchSnapshot();
  }, [fetchSnapshot]);

  const handleNavigate = useEffectEvent((page: unknown) => {
    if (isPage(page)) onNavigate(page);
  });

  const handleNotice = useEffectEvent((notice: Notice) => {
    notify(notice.level, notice.message);
  });

  // Backend events. In a plain browser there is no IPC bridge; the load error explains that.
  useEffect(() => {
    if (!isBackendAvailable()) return undefined;
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];
    const track = (registration: Promise<UnlistenFn>, topic: string): void => {
      registration.then(
        (unlisten) => {
          if (disposed) release(unlisten);
          else unlisteners.push(unlisten);
        },
        (error: unknown) => {
          if (!disposed) notifyError(error, `Live updates for ${topic} are unavailable`);
        },
      );
    };

    track(
      events.onManagedChanged((managed) => updateSnapshot((current) => ({ ...current, managed }))),
      'managed windows',
    );
    track(
      events.onSnapshot((next) => {
        setSnapshot(mergeSnapshot(snapshotRef.current, next));
        setLoad({ status: 'ready' });
      }),
      'settings',
    );
    track(events.onNotice((notice) => handleNotice(notice)), 'notifications');
    track(events.onNavigate((page) => handleNavigate(page)), 'navigation');

    return () => {
      disposed = true;
      for (const unlisten of unlisteners) release(unlisten);
    };
  }, [notifyError, setSnapshot, updateSnapshot]);

  // Do not lose debounced edits when the window is hidden or closed.
  useEffect(() => {
    const flush = (): void => {
      if (queue.hasPendingWork()) void queue.flush();
    };
    const onVisibilityChange = (): void => {
      if (document.visibilityState === 'hidden') flush();
    };
    window.addEventListener('pagehide', flush);
    document.addEventListener('visibilitychange', onVisibilityChange);
    return () => {
      window.removeEventListener('pagehide', flush);
      document.removeEventListener('visibilitychange', onVisibilityChange);
      flush();
    };
  }, [queue]);

  const editSettings = useCallback(
    (updater: SettingsUpdater) => {
      const base = draftRef.current ?? snapshotRef.current?.settings;
      if (!base) return;
      const next = updater(base);
      if (next === base) return;
      setDraft(next);
      queue.schedule(next);
    },
    [queue, setDraft],
  );

  const commitSettings = useCallback(
    async (updater: SettingsUpdater): Promise<boolean> => {
      const base = draftRef.current ?? snapshotRef.current?.settings;
      if (!base) return false;
      const next = updater(base);
      if (next === base) return true;
      setDraft(next);
      try {
        await queue.saveNow(next);
        return true;
      } catch {
        // Already reported to the user by the queue's onFailure handler (toast + indicator).
        return false;
      }
    },
    [queue, setDraft],
  );

  const flushSettings = useCallback(() => queue.flush(), [queue]);

  const updateManaged = useCallback(
    (update: (managed: ManagedWindow[]) => ManagedWindow[]) => {
      updateSnapshot((current) => ({ ...current, managed: update(current.managed) }));
    },
    [updateSnapshot],
  );

  const setManagementEnabled = useCallback(
    async (enabled: boolean): Promise<boolean> => {
      // Persist pending debounced edits first, so the toggle never races a settings save.
      await queue.flush();
      try {
        await api.setManagementEnabled(enabled);
      } catch (error) {
        notifyError(error, enabled ? 'Window management was not turned on' : 'Window management was not paused');
        return false;
      }
      updateSnapshot((current) => ({
        ...current,
        managementEnabled: enabled,
        settings: patchSection(current.settings, 'general', { managementEnabled: enabled }),
      }));
      // Shortcut registration changes with the master switch: fetch the new state.
      try {
        setSnapshot(await api.getSnapshot());
      } catch (error) {
        notifyError(error, 'Could not refresh the current state');
      }
      return true;
    },
    [notifyError, queue, setSnapshot, updateSnapshot],
  );

  const resetSettings = useCallback(async (): Promise<boolean> => {
    queue.cancelScheduled();
    await queue.whenIdle();
    try {
      const next = await api.resetSettings();
      setDraft(null);
      setSnapshot(mergeSnapshot(snapshotRef.current, next));
      setSaveError(null);
      notify('success', 'All settings were reset to their defaults.');
      return true;
    } catch (error) {
      notifyError(error, 'Settings were not reset');
      // Keep unsaved edits: schedule them again.
      const pending = draftRef.current;
      if (pending) queue.schedule(pending);
      return false;
    }
  }, [notify, notifyError, queue, setDraft, setSnapshot]);

  const exportSettings = useCallback(async (): Promise<boolean> => {
    try {
      // Export what the user sees: save pending edits first.
      await flushSettings();
      const path = await api.exportSettings();
      if (path === null) return false;
      notify('success', `Settings and rules were exported to ${path}.`);
      return true;
    } catch (error) {
      notifyError(error, 'Settings were not exported');
      return false;
    }
  }, [flushSettings, notify, notifyError]);

  const importSettings = useCallback(async (): Promise<boolean> => {
    queue.cancelScheduled();
    await queue.whenIdle();
    try {
      const next = await api.importSettings();
      if (next === null) {
        const pending = draftRef.current;
        if (pending) queue.schedule(pending);
        return false;
      }
      setDraft(null);
      setSnapshot(mergeSnapshot(snapshotRef.current, next));
      setSaveError(null);
      notify('success', `Settings imported: ${next.settings.rules.length} automatic rule(s).`);
      return true;
    } catch (error) {
      notifyError(error, 'Settings were not imported');
      const pending = draftRef.current;
      if (pending) queue.schedule(pending);
      return false;
    }
  }, [notify, notifyError, queue, setDraft, setSnapshot]);

  const dismissWarning = useCallback(() => {
    setDismissedWarning(snapshotRef.current?.settingsWarning ?? null);
  }, []);

  useDocumentTheme((draft ?? snapshot?.settings)?.general.theme ?? 'system');

  const value = useMemo<AppState | null>(() => {
    if (!snapshot) return null;
    return {
      snapshot,
      settings: draft ?? snapshot.settings,
      saveStatus,
      saveError,
      editSettings,
      commitSettings,
      flushSettings,
      updateManaged,
      setManagementEnabled,
      resetSettings,
      exportSettings,
      importSettings,
      dismissedWarning,
      dismissWarning,
    };
  }, [
    snapshot,
    draft,
    saveStatus,
    saveError,
    editSettings,
    commitSettings,
    flushSettings,
    updateManaged,
    setManagementEnabled,
    resetSettings,
    exportSettings,
    importSettings,
    dismissedWarning,
    dismissWarning,
  ]);

  if (load.status === 'error') {
    const available = isBackendAvailable();
    return (
      <StatusScreen
        kind="error"
        title={available ? 'AWWM did not respond' : BACKEND_UNAVAILABLE_TITLE}
        message={
          available
            ? 'The settings could not be loaded from the AWWM engine. Make sure AWWM is running, then try again.'
            : BACKEND_UNAVAILABLE_MESSAGE
        }
        detail={available ? errorMessage(load.error) : undefined}
        onRetry={retry}
      />
    );
  }

  if (load.status === 'loading' || value === null) {
    return <StatusScreen kind="loading" title="Loading AWWM…" />;
  }

  return <AppStateContext.Provider value={value}>{children}</AppStateContext.Provider>;
}
