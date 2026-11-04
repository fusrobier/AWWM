import { useCallback, useEffect, useSyncExternalStore } from 'react';
import { windowIcons } from '../lib/backend';

/** Icon data URL for an executable: `undefined` while loading, `null` when unavailable. */
export function useWindowIcon(exePath: string): string | null | undefined {
  const getSnapshot = useCallback(() => windowIcons.peek(exePath), [exePath]);
  const icon = useSyncExternalStore(windowIcons.subscribe, getSnapshot, getSnapshot);
  useEffect(() => {
    void windowIcons.load(exePath);
  }, [exePath]);
  return icon;
}
