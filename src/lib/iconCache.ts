// Per-executable icon cache. Pending requests are shared, results (including "no icon")
// are kept for the lifetime of the page, and subscribers are told when an icon arrives.

export type IconFetcher = (exePath: string) => Promise<string | null>;

export interface IconCache {
  /** Loads (once) and resolves with a data URL or null. Never rejects. */
  load(exePath: string): Promise<string | null>;
  /** `undefined` while unknown/loading, otherwise the cached result. */
  peek(exePath: string): string | null | undefined;
  subscribe(listener: () => void): () => void;
}

export function createIconCache(
  fetchIcon: IconFetcher,
  onError?: (exePath: string, error: unknown) => void,
): IconCache {
  const pending = new Map<string, Promise<string | null>>();
  const resolved = new Map<string, string | null>();
  const listeners = new Set<() => void>();

  // Windows paths are case-insensitive.
  const keyOf = (exePath: string): string => exePath.trim().toLowerCase();

  const notify = (): void => {
    for (const listener of listeners) listener();
  };

  return {
    load(exePath) {
      const key = keyOf(exePath);
      if (key === '') return Promise.resolve(null);
      const known = resolved.get(key);
      if (known !== undefined) return Promise.resolve(known);
      const inFlight = pending.get(key);
      if (inFlight) return inFlight;

      const request = fetchIcon(exePath)
        .then(
          (icon) => (typeof icon === 'string' && icon.length > 0 ? icon : null),
          (error: unknown) => {
            onError?.(exePath, error);
            return null;
          },
        )
        .then((icon) => {
          pending.delete(key);
          resolved.set(key, icon);
          notify();
          return icon;
        });
      pending.set(key, request);
      return request;
    },
    peek(exePath) {
      const key = keyOf(exePath);
      if (key === '') return null;
      return resolved.get(key);
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}
