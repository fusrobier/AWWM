// Rate-limiting helpers. They use the global timer functions and Date.now(), so tests can
// drive them with fake timers.

export interface RateLimited<A extends unknown[]> {
  (...args: A): void;
  /** Drops a call that is waiting to be executed. */
  cancel(): void;
  /** Executes a waiting call immediately. */
  flush(): void;
  /** True while a call is waiting to be executed. */
  isPending(): boolean;
}

type TimerHandle = ReturnType<typeof setTimeout>;

/** Calls `fn` once `waitMs` have passed without another call (trailing edge, last arguments). */
export function debounce<A extends unknown[]>(
  fn: (...args: A) => void,
  waitMs: number,
): RateLimited<A> {
  let timer: TimerHandle | undefined;
  let pendingArgs: A | undefined;

  const run = (): void => {
    timer = undefined;
    const args = pendingArgs;
    pendingArgs = undefined;
    if (args) fn(...args);
  };

  const debounced = (...args: A): void => {
    pendingArgs = args;
    if (timer !== undefined) clearTimeout(timer);
    timer = setTimeout(run, waitMs);
  };
  debounced.cancel = (): void => {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
    pendingArgs = undefined;
  };
  debounced.flush = (): void => {
    if (timer === undefined) return;
    clearTimeout(timer);
    run();
  };
  debounced.isPending = (): boolean => timer !== undefined;
  return debounced;
}

/**
 * Calls `fn` at most once per `intervalMs`: the first call runs immediately, later calls
 * inside the interval collapse into one trailing call with the latest arguments.
 */
export function throttle<A extends unknown[]>(
  fn: (...args: A) => void,
  intervalMs: number,
): RateLimited<A> {
  let lastRun = Number.NEGATIVE_INFINITY;
  let timer: TimerHandle | undefined;
  let pendingArgs: A | undefined;

  const invoke = (args: A): void => {
    lastRun = Date.now();
    fn(...args);
  };

  const runTrailing = (): void => {
    timer = undefined;
    const args = pendingArgs;
    pendingArgs = undefined;
    if (args) invoke(args);
  };

  const throttled = (...args: A): void => {
    const elapsed = Date.now() - lastRun;
    if (timer === undefined && elapsed >= intervalMs) {
      invoke(args);
      return;
    }
    pendingArgs = args;
    if (timer === undefined) {
      timer = setTimeout(runTrailing, Math.max(0, intervalMs - elapsed));
    }
  };
  throttled.cancel = (): void => {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
    pendingArgs = undefined;
  };
  throttled.flush = (): void => {
    if (timer === undefined) return;
    clearTimeout(timer);
    runTrailing();
  };
  throttled.isPending = (): boolean => timer !== undefined;
  return throttled;
}
