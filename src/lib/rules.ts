// Automation rule helpers: creation, validation, summaries and list edits.
import type { Rule, TitleMatcher, TitleMatchMode } from './types';
import { NBSP, formatPercent } from './format';
import { NEARLY_INVISIBLE_RULE_WARNING, isNearlyInvisible } from './opacity';
import { friendlyAppName } from './windows';

export const TITLE_MATCH_MODES: readonly TitleMatchMode[] = [
  'contains',
  'equals',
  'startsWith',
  'endsWith',
  'wildcard',
];

export const TITLE_MODE_LABELS: Record<TitleMatchMode, string> = {
  contains: 'Contains',
  equals: 'Is exactly',
  startsWith: 'Starts with',
  endsWith: 'Ends with',
  wildcard: 'Matches pattern (* and ?)',
};

const TITLE_MODE_PHRASES: Record<TitleMatchMode, string> = {
  contains: 'contains',
  equals: 'is',
  startsWith: 'starts with',
  endsWith: 'ends with',
  wildcard: 'matches',
};

/** Mirrors the backend's `normalize_exe_pattern`: trims whitespace and surrounding quotes. */
export function normalizeExePattern(raw: string): string {
  return raw.trim().replace(/^"+|"+$/g, '').trim();
}

export function createRule(newId: () => string = () => crypto.randomUUID()): Rule {
  return { id: newId(), name: '', enabled: true, exe: '', title: null, pin: true, opacity: 100 };
}

export function ruleHasEffect(rule: Pick<Rule, 'pin' | 'opacity'>): boolean {
  return rule.pin || rule.opacity < 100;
}

export interface RuleValidation {
  errors: { exe?: string; titlePattern?: string };
  warnings: string[];
  valid: boolean;
}

export function validateRule(rule: Rule): RuleValidation {
  const errors: RuleValidation['errors'] = {};
  const warnings: string[] = [];

  if (normalizeExePattern(rule.exe) === '') {
    errors.exe = 'Enter the executable to match, for example notepad.exe.';
  }
  if (rule.title !== null && rule.title.pattern.trim() === '') {
    errors.titlePattern = 'Enter the title text to match, or turn off the title condition.';
  }
  if (!ruleHasEffect(rule)) {
    warnings.push(
      'This rule has no effect: it neither pins windows nor changes their opacity.',
    );
  } else if (isNearlyInvisible(rule.opacity)) {
    warnings.push(NEARLY_INVISIBLE_RULE_WARNING);
  }

  return { errors, warnings, valid: errors.exe === undefined && errors.titlePattern === undefined };
}

/** Normalises the free-text fields before a rule is saved. */
export function finalizeRule(rule: Rule): Rule {
  const exe = normalizeExePattern(rule.exe);
  const name = rule.name.trim() || friendlyAppName(exe) || exe;
  return { ...rule, exe, name };
}

/** `Pin · 85 %`, `Pin`, `Opacity 70 %`, `No effect`. */
export function summarizeEffect(rule: Pick<Rule, 'pin' | 'opacity'>): string {
  const translucent = rule.opacity < 100;
  if (rule.pin && translucent) return `Pin${NBSP}·${NBSP}${formatPercent(rule.opacity)}`;
  if (rule.pin) return 'Pin';
  if (translucent) return `Opacity ${formatPercent(rule.opacity)}`;
  return 'No effect';
}

/** `Any title`, `Title contains “Untitled”`, `Title is “Calculator” (case-sensitive)`. */
export function summarizeTitleCondition(title: TitleMatcher | null): string {
  if (title === null || title.pattern === '') return 'Any title';
  const suffix = title.caseSensitive ? ' (case-sensitive)' : '';
  return `Title ${TITLE_MODE_PHRASES[title.mode]} “${title.pattern}”${suffix}`;
}

/** Fills a rule from a window chosen in the picker. */
export function applyWindowToRule(
  rule: Rule,
  window: { exeName: string; title: string },
  useTitle: boolean,
): Rule {
  const name = rule.name.trim() === '' ? friendlyAppName(window.exeName) : rule.name;
  const title: TitleMatcher | null =
    useTitle && window.title.trim() !== ''
      ? { mode: 'contains', pattern: window.title.trim(), caseSensitive: false }
      : rule.title;
  return { ...rule, exe: window.exeName, name, title };
}

/** Moves the rule at `index` by `delta` positions; out-of-range moves return the input. */
export function moveRule(rules: Rule[], index: number, delta: number): Rule[] {
  const target = index + delta;
  if (index < 0 || index >= rules.length || target < 0 || target >= rules.length || delta === 0) {
    return rules;
  }
  const next = [...rules];
  const [moved] = next.splice(index, 1);
  if (moved === undefined) return rules;
  next.splice(target, 0, moved);
  return next;
}

/** Replaces the rule with the same id, or appends a new one. */
export function upsertRule(rules: readonly Rule[], rule: Rule): Rule[] {
  const index = rules.findIndex((r) => r.id === rule.id);
  if (index === -1) return [...rules, rule];
  return rules.map((r, i) => (i === index ? rule : r));
}

export function removeRule(rules: readonly Rule[], id: string): Rule[] {
  return rules.filter((rule) => rule.id !== id);
}

export function setRuleEnabled(rules: readonly Rule[], id: string, enabled: boolean): Rule[] {
  return rules.map((rule) => (rule.id === id && rule.enabled !== enabled ? { ...rule, enabled } : rule));
}
