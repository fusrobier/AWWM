import { useLayoutEffect, useRef, useState } from 'react';
import { Button, IconButton } from '../components/Button';
import { Card } from '../components/Card';
import { ConfirmDialog } from '../components/Dialog';
import { Badge, EmptyState, InfoBar, PageHeader, SaveIndicator } from '../components/Feedback';
import { Switch } from '../components/Switch';
import { useCommandRunner } from '../hooks/useCommand';
import { api } from '../lib/api';
import { cx, pluralize } from '../lib/format';
import {
  createRule,
  finalizeRule,
  moveRule,
  removeRule,
  ruleHasEffect,
  setRuleEnabled,
  summarizeEffect,
  summarizeTitleCondition,
  upsertRule,
} from '../lib/rules';
import { setRules } from '../lib/settingsUpdate';
import type { Rule } from '../lib/types';
import { useAppState } from '../state/appStateContext';
import { useToasts } from '../state/toastContext';
import { RuleEditorDialog } from './RuleEditorDialog';

type MoveDirection = 'up' | 'down';

function ruleDisplayName(rule: Rule): string {
  return rule.name.trim() || rule.exe.trim() || 'Unnamed rule';
}

export function RulesPage() {
  const { snapshot, settings, commitSettings, flushSettings, saveStatus, saveError } = useAppState();
  const { notify } = useToasts();
  const { run, isBusy } = useCommandRunner();
  const [editing, setEditing] = useState<{ rule: Rule; isNew: boolean } | null>(null);
  const [deleting, setDeleting] = useState<Rule | null>(null);
  const pendingFocus = useRef<{ id: string; direction: MoveDirection } | null>(null);
  const listRef = useRef<HTMLOListElement>(null);
  const rules = settings.rules;

  // Moving a row re-orders the DOM; keep focus on the moved rule's buttons.
  useLayoutEffect(() => {
    const pending = pendingFocus.current;
    if (!pending) return;
    pendingFocus.current = null;
    const row = Array.from(listRef.current?.querySelectorAll<HTMLElement>('[data-rule-id]') ?? []).find(
      (element) => element.dataset.ruleId === pending.id,
    );
    if (!row) return;
    const target =
      row.querySelector<HTMLButtonElement>(`[data-move="${pending.direction}"]:not(:disabled)`) ??
      row.querySelector<HTMLButtonElement>('[data-move]:not(:disabled)') ??
      row.querySelector<HTMLButtonElement>('[data-edit]');
    target?.focus();
  }, [rules]);

  const move = (rule: Rule, direction: MoveDirection): void => {
    pendingFocus.current = { id: rule.id, direction };
    const delta = direction === 'up' ? -1 : 1;
    void commitSettings((current) => {
      const index = current.rules.findIndex((r) => r.id === rule.id);
      return setRules(current, moveRule(current.rules, index, delta));
    });
  };

  const setEnabled = (rule: Rule, enabled: boolean): void => {
    void commitSettings((current) => setRules(current, setRuleEnabled(current.rules, rule.id, enabled)));
  };

  const saveRule = (rule: Rule): Promise<boolean> =>
    commitSettings((current) => setRules(current, upsertRule(current.rules, finalizeRule(rule))));

  const deleteRule = (rule: Rule): Promise<boolean> =>
    commitSettings((current) => setRules(current, removeRule(current.rules, rule.id)));

  const applyNow = async (): Promise<void> => {
    await flushSettings();
    const result = await run('apply', () => api.applyRulesNow(), 'Rules were not applied');
    if (result.ok) notify('success', `Rules applied to ${pluralize(result.value, 'window')}.`);
  };

  return (
    <>
      <PageHeader
        title="Automatic rules"
        description="Pin windows or change their opacity automatically when they open."
        status={<SaveIndicator status={saveStatus} error={saveError} />}
        actions={
          <>
            <Button icon="play" busy={isBusy('apply')} onClick={() => void applyNow()}>
              Apply rules now
            </Button>
            <Button variant="accent" icon="plus" onClick={() => setEditing({ rule: createRule(), isNew: true })}>
              New rule
            </Button>
          </>
        }
      />

      {!snapshot.managementEnabled && (
        <InfoBar tone="warning" title="Window management is paused">
          Rules are not applied until window management is turned back on from the dashboard or the tray.
        </InfoBar>
      )}

      <Card
        title={
          <>
            Rules <span className="count-pill">{rules.length}</span>
          </>
        }
        description="Rules are checked from top to bottom. The first enabled rule that matches a window wins."
      >
        {rules.length === 0 ? (
          <EmptyState
            icon="rules"
            title="No automatic rules"
            actions={
              <Button variant="accent" icon="plus" onClick={() => setEditing({ rule: createRule(), isNew: true })}>
                New rule
              </Button>
            }
          >
            <p>For example, keep Calculator on top at 85 % opacity every time it opens.</p>
          </EmptyState>
        ) : (
          <ol ref={listRef} className="rule-list">
            {rules.map((rule, index) => {
              const name = ruleDisplayName(rule);
              const hasExe = rule.exe.trim() !== '';
              return (
                <li key={rule.id} className={cx('rule-row', !rule.enabled && 'is-off')} data-rule-id={rule.id}>
                  <span className="rule-order" aria-hidden="true">
                    {index + 1}
                  </span>
                  <Switch
                    checked={rule.enabled}
                    showState={false}
                    label={`Enable rule ${name}`}
                    onChange={(enabled) => setEnabled(rule, enabled)}
                  />
                  <div className="rule-main">
                    <p className="rule-name" title={name}>
                      {name}
                      <span className="sr-only">
                        , rule {index + 1} of {rules.length}
                      </span>
                    </p>
                    <p className="rule-meta">
                      {hasExe ? <code title={rule.exe}>{rule.exe}</code> : <span className="rule-missing">No executable</span>}
                      <span aria-hidden="true">·</span>
                      <span className="rule-title-condition" title={summarizeTitleCondition(rule.title)}>
                        {summarizeTitleCondition(rule.title)}
                      </span>
                    </p>
                  </div>
                  <Badge tone={ruleHasEffect(rule) ? 'accent' : 'warning'} icon={rule.pin ? 'pin' : 'opacity'}>
                    {summarizeEffect(rule)}
                  </Badge>
                  <div className="rule-actions">
                    <IconButton
                      icon="arrowUp"
                      label={`Move ${name} up`}
                      data-move="up"
                      disabled={index === 0}
                      onClick={() => move(rule, 'up')}
                    />
                    <IconButton
                      icon="arrowDown"
                      label={`Move ${name} down`}
                      data-move="down"
                      disabled={index === rules.length - 1}
                      onClick={() => move(rule, 'down')}
                    />
                    <IconButton
                      icon="edit"
                      label={`Edit ${name}`}
                      data-edit="true"
                      onClick={() => setEditing({ rule, isNew: false })}
                    />
                    <IconButton icon="trash" label={`Delete ${name}`} onClick={() => setDeleting(rule)} />
                  </div>
                </li>
              );
            })}
          </ol>
        )}
      </Card>

      <InfoBar>
        Windows 11 Calculator runs as <code>CalculatorApp.exe</code>. Packaged (Microsoft Store) apps are matched
        by their real executable, not by <code>ApplicationFrameHost.exe</code>.
      </InfoBar>

      {editing && (
        <RuleEditorDialog
          initial={editing.rule}
          isNew={editing.isNew}
          onClose={() => setEditing(null)}
          onSave={saveRule}
        />
      )}
      {deleting && (
        <ConfirmDialog
          title="Delete this rule?"
          message={`“${ruleDisplayName(deleting)}” will be removed from the rule list. This cannot be undone.`}
          confirmLabel="Delete"
          destructive
          onConfirm={() => deleteRule(deleting)}
          onClose={() => setDeleting(null)}
        />
      )}
    </>
  );
}
