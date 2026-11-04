/**
 * Memoises an async loader: concurrent and later calls share the first successful
 * promise. A failed load is forgotten so the next call retries.
 */
export function cacheOnSuccess<T>(load: () => Promise<T>): () => Promise<T> {
  let cached: Promise<T> | null = null;
  return () => {
    if (cached === null) {
      const attempt = load();
      cached = attempt;
      attempt.catch(() => {
        // The caller receives the rejection through the returned promise; here we only
        // make sure the next call starts a fresh attempt.
        if (cached === attempt) cached = null;
      });
    }
    return cached;
  };
}
