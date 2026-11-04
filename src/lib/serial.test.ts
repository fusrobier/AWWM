import { describe, expect, it } from 'vitest';
import { createLatestSerial } from './serial';
import { mergeSnapshot } from './snapshot';
import { applyManagedResult, isReleased } from './windows';
import { makeManaged, makeSettings } from '../test/fixtures';
import type { AppSnapshot } from './types';

interface Pending {
  value: number;
  resolve: (result: string) => void;
  reject: (error: unknown) => void;
}

function setup() {
  const pending: Pending[] = [];
  const serial = createLatestSerial<number, string>(
    (value) =>
      new Promise<string>((resolve, reject) => {
        pending.push({ value, resolve, reject });
      }),
  );
  const sent = (): number[] => pending.map((p) => p.value);
  const at = (index: number): Pending => {
    const p = pending[index];
    if (!p) throw new Error(`request #${index} was not sent`);
    return p;
  };
  return { serial, sent, at };
}

/** Lets queued promise callbacks run. */
const settle = async (): Promise<void> => {
  for (let i = 0; i < 5; i += 1) await Promise.resolve();
};

describe('createLatestSerial', () => {
  it('sends one request at a time and the commit only after earlier requests settle', async () => {
    const { serial, sent, at } = setup();
    void serial.offer(10);
    await settle();
    expect(sent()).toEqual([10]);

    const final = serial.commit(50);
    await settle();
    expect(sent()).toEqual([10]);

    at(0).resolve('ten');
    await settle();
    expect(sent()).toEqual([10, 50]);
    at(1).resolve('fifty');
    await expect(final).resolves.toBe('fifty');
  });

  it('skips intermediate values superseded before they started', async () => {
    const { serial, sent, at } = setup();
    void serial.offer(10);
    await settle();
    const skipped = serial.offer(20);
    const latest = serial.offer(30);
    at(0).resolve('ten');
    await expect(skipped).resolves.toBeUndefined();
    await settle();
    expect(sent()).toEqual([10, 30]);
    at(1).resolve('thirty');
    await expect(latest).resolves.toBe('thirty');
  });

  it('drops queued intermediate values when a commit is queued', async () => {
    const { serial, sent, at } = setup();
    void serial.offer(10);
    await settle();
    const stale = serial.offer(20);
    const final = serial.commit(40);
    at(0).resolve('ten');
    await expect(stale).resolves.toBeUndefined();
    await settle();
    expect(sent()).toEqual([10, 40]);
    at(1).resolve('forty');
    await expect(final).resolves.toBe('forty');
  });

  it('keeps going after a failed request', async () => {
    const { serial, sent, at } = setup();
    const failed = serial.offer(10);
    await settle();
    const final = serial.commit(60);
    at(0).reject(new Error('window closed'));
    await expect(failed).rejects.toThrow('window closed');
    await settle();
    expect(sent()).toEqual([10, 60]);
    at(1).resolve('sixty');
    await expect(final).resolves.toBe('sixty');
  });
});

describe('released managed windows', () => {
  it('treats unpinned, fully opaque results as released', () => {
    expect(isReleased({ pinned: false, opacity: 100 })).toBe(true);
    expect(isReleased({ pinned: false, opacity: 99 })).toBe(false);
    expect(isReleased({ pinned: true, opacity: 100 })).toBe(false);
  });

  it('removes a released window instead of adding a ghost row', () => {
    const list = [makeManaged({ hwnd: 1 }), makeManaged({ hwnd: 2, pinned: false, opacity: 60 })];
    const released = makeManaged({ hwnd: 2, pinned: false, opacity: 100 });
    expect(applyManagedResult(list, released).map((w) => w.hwnd)).toEqual([1]);
    // Already gone (managed-changed arrived first): stays gone.
    expect(applyManagedResult([makeManaged({ hwnd: 1 })], released).map((w) => w.hwnd)).toEqual([1]);
  });

  it('upserts windows that are still managed', () => {
    const list = [makeManaged({ hwnd: 1 })];
    expect(applyManagedResult(list, makeManaged({ hwnd: 1, opacity: 40 }))[0]?.opacity).toBe(40);
    expect(applyManagedResult(list, makeManaged({ hwnd: 3, pinned: false, opacity: 70 }))).toHaveLength(2);
  });
});

describe('mergeSnapshot', () => {
  const snapshot = (managed: AppSnapshot['managed'], theme: 'light' | 'dark'): AppSnapshot => {
    const settings = makeSettings();
    return {
      managementEnabled: true,
      managed,
      settings: { ...settings, general: { ...settings.general, theme } },
      hotkeys: [],
      settingsWarning: null,
    };
  };

  it('takes settings from the incoming snapshot but keeps the current managed list', () => {
    const current = snapshot([makeManaged({ hwnd: 1 }), makeManaged({ hwnd: 2 })], 'light');
    const incoming = snapshot([makeManaged({ hwnd: 1 })], 'dark');
    const merged = mergeSnapshot(current, incoming);
    expect(merged.settings.general.theme).toBe('dark');
    expect(merged.managed).toBe(current.managed);
  });

  it('uses the incoming snapshot completely before the first load', () => {
    const incoming = snapshot([makeManaged({ hwnd: 7 })], 'dark');
    expect(mergeSnapshot(null, incoming)).toBe(incoming);
  });
});
