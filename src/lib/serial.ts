// Ordered request chains for controls that send a stream of values (opacity sliders).

export interface LatestSerial<T, R> {
  /**
   * Queues an intermediate value. It is sent after every earlier request has settled,
   * unless a newer `offer` or a `commit` was queued before it started; then it is
   * skipped and resolves with `undefined`. This keeps the chain short while dragging.
   */
  offer(value: T): Promise<R | undefined>;
  /**
   * Queues a final value that is always sent, strictly after every earlier request has
   * settled, so it can never be overtaken by an older intermediate value.
   */
  commit(value: T): Promise<R>;
}

export function createLatestSerial<T, R>(send: (value: T) => Promise<R>): LatestSerial<T, R> {
  let tail: Promise<unknown> = Promise.resolve();
  let generation = 0;

  const enqueue = <X>(task: () => Promise<X>): Promise<X> => {
    // Run after the previous request settles, whether it succeeded or failed.
    const result = tail.then(task, task);
    tail = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  };

  return {
    offer(value) {
      generation += 1;
      const mine = generation;
      return enqueue(async () => (mine === generation ? send(value) : undefined));
    },
    commit(value) {
      generation += 1;
      return enqueue(() => send(value));
    },
  };
}
