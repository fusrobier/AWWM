// Serialised, debounced persistence. Used for settings: text and number edits are
// scheduled (debounced), toggles and selects are saved immediately. Saves never overlap,
// so the backend always receives them in order; values queued while a save is in flight
// are coalesced into a single follow-up save of the latest value.

export type SaveStatus = 'idle' | 'pending' | 'saving' | 'saved' | 'error';

interface Waiter<R> {
  resolve: (result: R) => void;
  reject: (error: unknown) => void;
}

export interface SaveQueueOptions<T, R> {
  save: (value: T) => Promise<R>;
  /** Debounce delay for `schedule()`. */
  delayMs: number;
  /**
   * `superseded` is true when a newer value is already waiting, i.e. `value` is no longer
   * the latest edit.
   */
  onSuccess?: (result: R, value: T, superseded: boolean) => void;
  onFailure?: (error: unknown, value: T, superseded: boolean) => void;
  onStatusChange?: (status: SaveStatus) => void;
}

export class SaveQueue<T, R> {
  private readonly options: SaveQueueOptions<T, R>;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private scheduled: { value: T } | null = null;
  private queued: { value: T; waiters: Waiter<R>[] } | null = null;
  private running = false;
  private idleWaiters: Array<() => void> = [];
  private status: SaveStatus = 'idle';

  constructor(options: SaveQueueOptions<T, R>) {
    this.options = options;
  }

  getStatus(): SaveStatus {
    return this.status;
  }

  /** True while a save is scheduled, queued or in flight. */
  hasPendingWork(): boolean {
    return this.running || this.scheduled !== null || this.queued !== null;
  }

  /** Debounced save of `value` (replaces any previously scheduled value). */
  schedule(value: T): void {
    this.scheduled = { value };
    if (this.timer !== undefined) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = undefined;
      const scheduled = this.scheduled;
      this.scheduled = null;
      if (scheduled) this.enqueue(scheduled.value, null);
    }, this.options.delayMs);
    if (!this.running) this.setStatus('pending');
  }

  /**
   * Saves `value` as soon as possible. A scheduled value is dropped because `value` is
   * expected to include it. Resolves with the result of the save that persisted `value`
   * (or a newer value that superseded it).
   */
  saveNow(value: T): Promise<R> {
    this.clearScheduled();
    return new Promise<R>((resolve, reject) => {
      this.enqueue(value, { resolve, reject });
    });
  }

  /** Saves a scheduled value immediately and resolves once all saves have finished. */
  flush(): Promise<void> {
    const scheduled = this.scheduled;
    if (scheduled) {
      this.clearScheduled();
      this.enqueue(scheduled.value, null);
    }
    return this.whenIdle();
  }

  /** Drops a scheduled (not yet started) save. */
  cancelScheduled(): void {
    const hadScheduled = this.scheduled !== null;
    this.clearScheduled();
    if (hadScheduled && !this.running) this.setStatus('idle');
  }

  /** Resolves once no save is in flight or queued. */
  whenIdle(): Promise<void> {
    if (!this.running && this.queued === null) return Promise.resolve();
    return new Promise<void>((resolve) => {
      this.idleWaiters.push(resolve);
    });
  }

  private clearScheduled(): void {
    if (this.timer !== undefined) clearTimeout(this.timer);
    this.timer = undefined;
    this.scheduled = null;
  }

  private enqueue(value: T, waiter: Waiter<R> | null): void {
    if (this.running) {
      if (this.queued) {
        this.queued.value = value;
        if (waiter) this.queued.waiters.push(waiter);
      } else {
        this.queued = { value, waiters: waiter ? [waiter] : [] };
      }
      return;
    }
    void this.run(value, waiter ? [waiter] : []);
  }

  private async run(value: T, waiters: Waiter<R>[]): Promise<void> {
    this.running = true;
    this.setStatus('saving');

    let outcome: { ok: true; result: R } | { ok: false; error: unknown };
    try {
      outcome = { ok: true, result: await this.options.save(value) };
    } catch (error) {
      outcome = { ok: false, error };
    }

    const superseded = this.queued !== null || this.scheduled !== null;
    if (outcome.ok) {
      this.options.onSuccess?.(outcome.result, value, superseded);
      for (const waiter of waiters) waiter.resolve(outcome.result);
    } else {
      this.options.onFailure?.(outcome.error, value, superseded);
      for (const waiter of waiters) waiter.reject(outcome.error);
    }

    this.running = false;
    const next = this.queued;
    if (next) {
      this.queued = null;
      void this.run(next.value, next.waiters);
      return;
    }

    if (this.scheduled) this.setStatus('pending');
    else this.setStatus(outcome.ok ? 'saved' : 'error');

    const idle = this.idleWaiters;
    this.idleWaiters = [];
    for (const resolve of idle) resolve();
  }

  private setStatus(status: SaveStatus): void {
    if (this.status === status) return;
    this.status = status;
    this.options.onStatusChange?.(status);
  }
}
