import { describe, expect, it, vi } from 'vitest';
import { cacheOnSuccess } from './cache';
import { cx, formatPercent, isHexColor, pluralize } from './format';
import { createIconCache } from './iconCache';
import { initialPage, isPage, nextIndex, pageFromHash } from './navigation';
import { addToast, removeToast, TOAST_DURATION_MS, type ToastData } from './notifications';
import { clamp, interpretIntegerInput } from './numbers';
import { clampOpacity, isNearlyInvisible } from './opacity';
import { resolveTheme } from './theme';

describe('format', () => {
  it('pluralises and formats percentages', () => {
    expect(pluralize(1, 'window')).toBe('1 window');
    expect(pluralize(3, 'window')).toBe('3 windows');
    expect(pluralize(0, 'entry', 'entries')).toBe('0 entries');
    expect(formatPercent(84.6)).toBe('85\u00A0%');
    expect(cx('a', false, null, undefined, '', 'b')).toBe('a b');
  });

  it('accepts only #RRGGBB colours', () => {
    expect(isHexColor('#3B82F6')).toBe(true);
    expect(isHexColor('#3b82f6')).toBe(true);
    expect(isHexColor('3B82F6')).toBe(false);
    expect(isHexColor('#3B82F')).toBe(false);
    expect(isHexColor('#GG82F6')).toBe(false);
  });
});

describe('numbers', () => {
  it('clamps and interprets integer input', () => {
    expect(clamp(5, 1, 3)).toBe(3);
    expect(clamp(Number.NaN, 1, 3)).toBe(1);
    expect(interpretIntegerInput(' 12 ', 2, 24)).toEqual({ kind: 'valid', value: 12 });
    expect(interpretIntegerInput('99', 2, 24)).toEqual({ kind: 'outOfRange', value: 99, clamped: 24 });
    expect(interpretIntegerInput('-4', 0, 600)).toEqual({ kind: 'outOfRange', value: -4, clamped: 0 });
    expect(interpretIntegerInput('', 0, 10)).toEqual({ kind: 'empty' });
    expect(interpretIntegerInput('2.5', 0, 10)).toEqual({ kind: 'invalid' });
    expect(interpretIntegerInput('1e3', 0, 10)).toEqual({ kind: 'invalid' });
  });
});

describe('opacity', () => {
  it('flags windows below 10 % as nearly invisible', () => {
    expect(isNearlyInvisible(9)).toBe(true);
    expect(isNearlyInvisible(0)).toBe(true);
    expect(isNearlyInvisible(10)).toBe(false);
    expect(clampOpacity(104.2)).toBe(100);
    expect(clampOpacity(-3)).toBe(0);
  });
});

describe('theme', () => {
  it('follows the system preference only for "system"', () => {
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
    expect(resolveTheme('light', true)).toBe('light');
    expect(resolveTheme('dark', false)).toBe('dark');
  });
});

describe('navigation', () => {
  it('validates pages from tray events', () => {
    expect(isPage('rules')).toBe(true);
    expect(isPage('settings')).toBe(false);
    expect(isPage(42)).toBe(false);
  });

  it('reads the initial page from the URL fragment', () => {
    expect(pageFromHash('#rules')).toBe('rules');
    expect(pageFromHash('#dashboard')).toBe('dashboard');
    expect(pageFromHash('mouse')).toBe('mouse');
    expect(pageFromHash('')).toBeNull();
    expect(pageFromHash('#')).toBeNull();
    expect(pageFromHash('#settings')).toBeNull();
    expect(pageFromHash('#Rules')).toBeNull();
    expect(initialPage('#about')).toBe('about');
    expect(initialPage('#unknown')).toBe('dashboard');
    expect(initialPage('')).toBe('dashboard');
  });

  it('computes arrow-key targets with wrap-around', () => {
    expect(nextIndex(0, 5, 'ArrowDown')).toBe(1);
    expect(nextIndex(4, 5, 'ArrowDown')).toBe(0);
    expect(nextIndex(0, 5, 'ArrowUp')).toBe(4);
    expect(nextIndex(2, 5, 'End')).toBe(4);
    expect(nextIndex(2, 5, 'Home')).toBe(0);
    expect(nextIndex(2, 5, 'Tab')).toBeNull();
    expect(nextIndex(0, 0, 'ArrowDown')).toBeNull();
  });
});

describe('toasts', () => {
  const toast = (id: number, level: ToastData['level'], message: string) => ({ id, level, message, title: null });

  it('keeps errors visible longer', () => {
    expect(TOAST_DURATION_MS.info).toBe(5000);
    expect(TOAST_DURATION_MS.error).toBe(8000);
  });

  it('refreshes duplicates instead of stacking them', () => {
    const one = addToast([], toast(1, 'info', 'Saved'));
    const two = addToast(one, toast(2, 'info', 'Saved'));
    expect(two).toHaveLength(1);
    expect(two[0]?.revision).toBe(1);
    expect(two[0]?.id).toBe(1);
  });

  it('drops the oldest non-error toast when full', () => {
    let list: ToastData[] = [];
    list = addToast(list, toast(1, 'error', 'e1'), 3);
    list = addToast(list, toast(2, 'info', 'i1'), 3);
    list = addToast(list, toast(3, 'info', 'i2'), 3);
    list = addToast(list, toast(4, 'success', 's1'), 3);
    expect(list.map((t) => t.id)).toEqual([1, 3, 4]);
    expect(removeToast(list, 3).map((t) => t.id)).toEqual([1, 4]);
  });
});

describe('icon cache', () => {
  it('shares pending requests and caches results per path (case-insensitive)', async () => {
    const fetchIcon = vi.fn((path: string) => Promise.resolve(`data:image/png;base64,${path.length}`));
    const cache = createIconCache(fetchIcon);
    const listener = vi.fn();
    cache.subscribe(listener);

    const a = cache.load('C:\\App\\a.exe');
    const b = cache.load('c:\\app\\A.EXE');
    expect(a).toBe(b);
    expect(cache.peek('C:\\App\\a.exe')).toBeUndefined();
    await a;
    expect(fetchIcon).toHaveBeenCalledTimes(1);
    expect(cache.peek('C:\\APP\\A.exe')).toMatch(/^data:image\/png/);
    expect(listener).toHaveBeenCalledTimes(1);
    await cache.load('C:\\App\\a.exe');
    expect(fetchIcon).toHaveBeenCalledTimes(1);
  });

  it('resolves to null for failures and empty paths without rejecting', async () => {
    const onError = vi.fn();
    const cache = createIconCache(() => Promise.reject(new Error('boom')), onError);
    await expect(cache.load('C:\\x.exe')).resolves.toBeNull();
    expect(onError).toHaveBeenCalledTimes(1);
    expect(cache.peek('C:\\x.exe')).toBeNull();
    await expect(cache.load('')).resolves.toBeNull();
  });
});

describe('cacheOnSuccess', () => {
  it('memoises success and retries after failure', async () => {
    let attempt = 0;
    const load = cacheOnSuccess(() => {
      attempt += 1;
      return attempt === 1 ? Promise.reject(new Error('offline')) : Promise.resolve(attempt);
    });
    await expect(load()).rejects.toThrow('offline');
    await expect(load()).resolves.toBe(2);
    await expect(load()).resolves.toBe(2);
    expect(attempt).toBe(2);
  });
});
