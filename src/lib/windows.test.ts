import { describe, expect, it } from 'vitest';
import {
  displayTitle,
  filterWindows,
  friendlyAppName,
  managedSourceLabel,
  managedSourceTone,
  removeManaged,
  upsertManaged,
} from './windows';
import { makeManaged, makeRule } from '../test/fixtures';

const windows = [
  { title: 'Untitled - Notepad', exeName: 'notepad.exe' },
  { title: 'Calculator', exeName: 'CalculatorApp.exe' },
  { title: 'README.md - Visual Studio Code', exeName: 'Code.exe' },
];

describe('filterWindows', () => {
  it('matches title or executable, case-insensitively, all terms', () => {
    expect(filterWindows(windows, '')).toHaveLength(3);
    expect(filterWindows(windows, 'NOTEPAD').map((w) => w.exeName)).toEqual(['notepad.exe']);
    expect(filterWindows(windows, 'calculatorapp').map((w) => w.title)).toEqual(['Calculator']);
    expect(filterWindows(windows, 'readme code').map((w) => w.exeName)).toEqual(['Code.exe']);
    expect(filterWindows(windows, 'readme notepad')).toEqual([]);
  });
});

describe('names', () => {
  it('derives readable names', () => {
    expect(friendlyAppName('notepad.exe')).toBe('Notepad');
    expect(friendlyAppName('C:\\Tools\\vlc.EXE')).toBe('Vlc');
    expect(friendlyAppName('')).toBe('');
    expect(displayTitle('   ')).toBe('Untitled window');
    expect(displayTitle(' Calculator ')).toBe('Calculator');
  });
});

describe('managed source', () => {
  it('labels manual, rule and recovered windows', () => {
    const rules = [makeRule({ id: 'r1', name: 'Notes' })];
    expect(managedSourceLabel({ source: 'manual', ruleId: null }, rules)).toBe('Manual');
    expect(managedSourceLabel({ source: 'recovered', ruleId: null }, rules)).toBe('Recovered');
    expect(managedSourceLabel({ source: 'rule', ruleId: 'r1' }, rules)).toBe('Rule: Notes');
    expect(managedSourceLabel({ source: 'rule', ruleId: 'gone' }, rules)).toBe('Rule (deleted)');
    expect(managedSourceTone('rule')).toBe('accent');
  });

  it('upserts and removes managed windows by handle', () => {
    const list = [makeManaged({ hwnd: 1 }), makeManaged({ hwnd: 2 })];
    const updated = upsertManaged(list, makeManaged({ hwnd: 2, opacity: 40 }));
    expect(updated[1]?.opacity).toBe(40);
    expect(updated[0]).toBe(list[0]);
    expect(upsertManaged(list, makeManaged({ hwnd: 3 }))).toHaveLength(3);
    expect(removeManaged(list, 1).map((w) => w.hwnd)).toEqual([2]);
  });
});
