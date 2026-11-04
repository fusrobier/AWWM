import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SaveQueue, type SaveStatus } from './saveQueue';

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (error: unknown) => void;
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function setup() {
  const calls: string[] = [];
  const replies: Array<Deferred<string>> = [];
  const statuses: SaveStatus[] = [];
  const successes: Array<{ result: string; value: string; superseded: boolean }> = [];
  const failures: Array<{ value: string; superseded: boolean }> = [];
  const queue = new SaveQueue<string, string>({
    delayMs: 400,
    save: (value) => {
      calls.push(value);
      const reply = deferred<string>();
      replies.push(reply);
      return reply.promise;
    },
    onSuccess: (result, value, superseded) => successes.push({ result, value, superseded }),
    onFailure: (_error, value, superseded) => failures.push({ value, superseded }),
    onStatusChange: (status) => statuses.push(status),
  });
  const reply = (index: number): Deferred<string> => {
    const r = replies[index];
    if (!r) throw new Error(`no save #${index}`);
    return r;
  };
  return { queue, calls, reply, statuses, successes, failures };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('SaveQueue', () => {
  it('debounces scheduled values and saves only the latest', async () => {
    const { queue, calls, reply, statuses } = setup();
    queue.schedule('a');
    vi.advanceTimersByTime(300);
    queue.schedule('b');
    vi.advanceTimersByTime(399);
    expect(calls).toEqual([]);
    expect(queue.getStatus()).toBe('pending');
    vi.advanceTimersByTime(1);
    expect(calls).toEqual(['b']);
    reply(0).resolve('B');
    await queue.whenIdle();
    expect(statuses).toEqual(['pending', 'saving', 'saved']);
  });

  it('saveNow replaces a scheduled value and resolves with the result', async () => {
    const { queue, calls, reply } = setup();
    queue.schedule('draft');
    const saved = queue.saveNow('final');
    expect(calls).toEqual(['final']);
    vi.advanceTimersByTime(1000);
    expect(calls).toEqual(['final']);
    reply(0).resolve('FINAL');
    await expect(saved).resolves.toBe('FINAL');
  });

  it('never overlaps saves and coalesces values queued meanwhile', async () => {
    const { queue, calls, reply, successes } = setup();
    const first = queue.saveNow('a');
    const second = queue.saveNow('b');
    const third = queue.saveNow('c');
    expect(calls).toEqual(['a']);

    reply(0).resolve('A');
    await expect(first).resolves.toBe('A');
    expect(calls).toEqual(['a', 'c']);
    expect(successes[0]).toEqual({ result: 'A', value: 'a', superseded: true });

    reply(1).resolve('C');
    await expect(second).resolves.toBe('C');
    await expect(third).resolves.toBe('C');
    expect(successes[1]).toEqual({ result: 'C', value: 'c', superseded: false });
  });

  it('reports failures to the callback and the waiting caller', async () => {
    const { queue, reply, failures } = setup();
    const saved = queue.saveNow('x');
    reply(0).reject({ code: 'io', message: 'disk full' });
    await expect(saved).rejects.toEqual({ code: 'io', message: 'disk full' });
    expect(failures).toEqual([{ value: 'x', superseded: false }]);
    expect(queue.getStatus()).toBe('error');
  });

  it('flush saves a scheduled value immediately and waits for it', async () => {
    const { queue, calls, reply } = setup();
    queue.schedule('pending');
    const flushed = queue.flush();
    expect(calls).toEqual(['pending']);
    let done = false;
    void flushed.then(() => {
      done = true;
    });
    await Promise.resolve();
    expect(done).toBe(false);
    reply(0).resolve('PENDING');
    await flushed;
    expect(queue.hasPendingWork()).toBe(false);
    expect(queue.getStatus()).toBe('saved');
  });

  it('cancelScheduled drops a scheduled save', () => {
    const { queue, calls } = setup();
    queue.schedule('discard');
    queue.cancelScheduled();
    vi.advanceTimersByTime(1000);
    expect(calls).toEqual([]);
    expect(queue.getStatus()).toBe('idle');
  });
});
