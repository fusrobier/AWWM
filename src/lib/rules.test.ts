import { describe, expect, it } from 'vitest';
import {
  applyWindowToRule,
  createRule,
  finalizeRule,
  moveRule,
  normalizeExePattern,
  removeRule,
  setRuleEnabled,
  summarizeEffect,
  summarizeTitleCondition,
  upsertRule,
  validateRule,
} from './rules';
import { makeRule } from '../test/fixtures';

const NBSP = '\u00A0';

describe('createRule', () => {
  it('creates an enabled pin rule with a fresh id', () => {
    const rule = createRule(() => 'id-1');
    expect(rule).toEqual({
      id: 'id-1',
      name: '',
      enabled: true,
      exe: '',
      title: null,
      pin: true,
      opacity: 100,
    });
  });

  it('uses crypto.randomUUID by default', () => {
    const a = createRule();
    const b = createRule();
    expect(a.id).toMatch(/^[0-9a-f-]{36}$/);
    expect(a.id).not.toBe(b.id);
  });
});

describe('validateRule', () => {
  it('requires an executable', () => {
    const result = validateRule(makeRule({ exe: '  "" ' }));
    expect(result.valid).toBe(false);
    expect(result.errors.exe).toBeDefined();
  });

  it('requires a title pattern when a title condition is set', () => {
    const result = validateRule(
      makeRule({ title: { mode: 'contains', pattern: '   ', caseSensitive: false } }),
    );
    expect(result.valid).toBe(false);
    expect(result.errors.titlePattern).toBeDefined();
  });

  it('warns about rules without effect but still accepts them', () => {
    const result = validateRule(makeRule({ pin: false, opacity: 100 }));
    expect(result.valid).toBe(true);
    expect(result.warnings).toHaveLength(1);
    expect(result.warnings[0]).toContain('no effect');
  });

  it('warns when matching windows would be nearly invisible', () => {
    const result = validateRule(makeRule({ pin: false, opacity: 5 }));
    expect(result.valid).toBe(true);
    expect(result.warnings[0]).toContain('nearly invisible');
    expect(validateRule(makeRule({ opacity: 10 })).warnings).toEqual([]);
  });
});

describe('summaries', () => {
  it('summarises the effect', () => {
    expect(summarizeEffect({ pin: true, opacity: 85 })).toBe(`Pin${NBSP}·${NBSP}85${NBSP}%`);
    expect(summarizeEffect({ pin: true, opacity: 100 })).toBe('Pin');
    expect(summarizeEffect({ pin: false, opacity: 70 })).toBe(`Opacity 70${NBSP}%`);
    expect(summarizeEffect({ pin: false, opacity: 100 })).toBe('No effect');
  });

  it('summarises the title condition', () => {
    expect(summarizeTitleCondition(null)).toBe('Any title');
    expect(summarizeTitleCondition({ mode: 'contains', pattern: 'Untitled', caseSensitive: false })).toBe(
      'Title contains “Untitled”',
    );
    expect(summarizeTitleCondition({ mode: 'equals', pattern: 'Calculator', caseSensitive: true })).toBe(
      'Title is “Calculator” (case-sensitive)',
    );
    expect(summarizeTitleCondition({ mode: 'wildcard', pattern: '*.txt - Notepad', caseSensitive: false })).toBe(
      'Title matches “*.txt - Notepad”',
    );
  });
});

describe('editing helpers', () => {
  it('normalises executable patterns like the backend', () => {
    expect(normalizeExePattern('  "notepad.exe" ')).toBe('notepad.exe');
    expect(normalizeExePattern('C:\\Tools\\app.exe')).toBe('C:\\Tools\\app.exe');
  });

  it('finalises names and executables before saving', () => {
    expect(finalizeRule(makeRule({ name: '  ', exe: ' "notepad.exe" ' }))).toMatchObject({
      name: 'Notepad',
      exe: 'notepad.exe',
    });
    expect(finalizeRule(makeRule({ name: ' Editor ', exe: 'code.exe' })).name).toBe('Editor');
  });

  it('fills a rule from a picked window', () => {
    const window = { exeName: 'CalculatorApp.exe', title: 'Calculator' };
    const blank = createRule(() => 'x');
    const filled = applyWindowToRule(blank, window, false);
    expect(filled).toMatchObject({ exe: 'CalculatorApp.exe', name: 'CalculatorApp', title: null });

    const named = applyWindowToRule({ ...blank, name: 'Calc' }, window, true);
    expect(named.name).toBe('Calc');
    expect(named.title).toEqual({ mode: 'contains', pattern: 'Calculator', caseSensitive: false });
  });

  it('moves rules up and down without mutating the input', () => {
    const rules = [makeRule({ id: 'a' }), makeRule({ id: 'b' }), makeRule({ id: 'c' })];
    expect(moveRule(rules, 2, -1).map((r) => r.id)).toEqual(['a', 'c', 'b']);
    expect(moveRule(rules, 0, 1).map((r) => r.id)).toEqual(['b', 'a', 'c']);
    expect(moveRule(rules, 0, -1)).toBe(rules);
    expect(moveRule(rules, 2, 1)).toBe(rules);
    expect(rules.map((r) => r.id)).toEqual(['a', 'b', 'c']);
  });

  it('upserts, removes and toggles rules', () => {
    const rules = [makeRule({ id: 'a' }), makeRule({ id: 'b' })];
    expect(upsertRule(rules, makeRule({ id: 'b', name: 'B2' }))[1]?.name).toBe('B2');
    expect(upsertRule(rules, makeRule({ id: 'c' }))).toHaveLength(3);
    expect(removeRule(rules, 'a').map((r) => r.id)).toEqual(['b']);
    const toggled = setRuleEnabled(rules, 'a', false);
    expect(toggled[0]?.enabled).toBe(false);
    expect(toggled[1]).toBe(rules[1]);
  });
});
