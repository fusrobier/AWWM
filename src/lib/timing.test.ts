import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { debounce, throttle } from './timing';

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(0);
});

afterEach(() => {
  vi.useRealTimers();
});

describe('debounce', () => {
  it('calls once with the latest arguments after the quiet period', () => {
    const fn = vi.fn<(value: number) => void>();
    const debounced = debounce(fn, 400);
    debounced(1);
    vi.advanceTimersByTime(200);
    debounced(2);
    vi.advanceTimersByTime(399);
    expect(fn).not.toHaveBeenCalled();
    expect(debounced.isPending()).toBe(true);
    vi.advanceTimersByTime(1);
    expect(fn).toHaveBeenCalledTimes(1);
    expect(fn).toHaveBeenCalledWith(2);
    expect(debounced.isPending()).toBe(false);
  });

  it('can be cancelled and flushed', () => {
    const fn = vi.fn<(value: string) => void>();
    const debounced = debounce(fn, 100);
    debounced('a');
    debounced.cancel();
    vi.advanceTimersByTime(500);
    expect(fn).not.toHaveBeenCalled();

    debounced('b');
    debounced.flush();
    expect(fn).toHaveBeenCalledWith('b');
    debounced.flush();
    expect(fn).toHaveBeenCalledTimes(1);
  });
});

describe('throttle', () => {
  it('runs the first call immediately and coalesces the rest into one trailing call', () => {
    const fn = vi.fn<(value: number) => void>();
    const throttled = throttle(fn, 40);
    throttled(1);
    expect(fn).toHaveBeenLastCalledWith(1);

    vi.advanceTimersByTime(10);
    throttled(2);
    vi.advanceTimersByTime(10);
    throttled(3);
    expect(fn).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(20);
    expect(fn).toHaveBeenCalledTimes(2);
    expect(fn).toHaveBeenLastCalledWith(3);
  });

  it('waits for the remainder of the interval after a trailing call', () => {
    const fn = vi.fn<(value: number) => void>();
    const throttled = throttle(fn, 40);
    throttled(1);
    vi.advanceTimersByTime(45);
    throttled(2);
    expect(fn).toHaveBeenCalledTimes(2);
    vi.advanceTimersByTime(5);
    throttled(3);
    vi.advanceTimersByTime(34);
    expect(fn).toHaveBeenCalledTimes(2);
    vi.advanceTimersByTime(1);
    expect(fn).toHaveBeenLastCalledWith(3);
  });

  it('cancel drops the trailing call and flush runs it now', () => {
    const fn = vi.fn<(value: number) => void>();
    const throttled = throttle(fn, 40);
    throttled(1);
    throttled(2);
    throttled.cancel();
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(1);

    throttled(3);
    throttled(4);
    expect(throttled.isPending()).toBe(true);
    throttled.flush();
    expect(fn).toHaveBeenLastCalledWith(4);
    expect(throttled.isPending()).toBe(false);
  });
});
