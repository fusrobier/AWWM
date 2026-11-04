import { useId, useRef, useState } from 'react';
import { Button } from '../components/Button';
import { Checkbox } from '../components/Checkbox';
import { Dialog } from '../components/Dialog';
import { InfoBar } from '../components/Feedback';
import { Field } from '../components/Field';
import { Select } from '../components/Select';
import { Slider } from '../components/Slider';
import { TextInput } from '../components/TextInput';
import { WindowPicker } from '../components/WindowPicker';
import { applyWindowToRule, TITLE_MATCH_MODES, TITLE_MODE_LABELS, validateRule } from '../lib/rules';
import type { Rule, TitleMatcher, WindowInfo } from '../lib/types';
import { displayTitle } from '../lib/windows';
import { useAppState } from '../state/appStateContext';

const TITLE_MODE_OPTIONS = TITLE_MATCH_MODES.map((mode) => ({ value: mode, label: TITLE_MODE_LABELS[mode] }));

const EMPTY_TITLE: TitleMatcher = { mode: 'contains', pattern: '', caseSensitive: false };

interface RuleEditorDialogProps {
  initial: Rule;
  isNew: boolean;
  onClose: () => void;
  /** Persists the rule; resolves to success. */
  onSave: (rule: Rule) => Promise<boolean>;
}

export function RuleEditorDialog({ initial, isNew, onClose, onSave }: RuleEditorDialogProps) {
  const { saveError } = useAppState();
  const [rule, setRule] = useState<Rule>(initial);
  const [submitted, setSubmitted] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveFailed, setSaveFailed] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [picked, setPicked] = useState<WindowInfo | null>(null);
  const lastTitle = useRef<TitleMatcher>(initial.title ?? EMPTY_TITLE);
  const exeRef = useRef<HTMLInputElement>(null);
  const patternRef = useRef<HTMLInputElement>(null);
  const ids = {
    name: useId(),
    nameHint: useId(),
    exe: useId(),
    exeHint: useId(),
    exeError: useId(),
    mode: useId(),
    pattern: useId(),
    patternError: useId(),
    opacity: useId(),
    titleSection: useId(),
    effectSection: useId(),
  };

  const validation = validateRule(rule);
  const exeError = submitted ? validation.errors.exe : undefined;
  const patternError = submitted ? validation.errors.titlePattern : undefined;

  const update = (patch: Partial<Rule>): void => setRule((current) => ({ ...current, ...patch }));

  const updateTitle = (patch: Partial<TitleMatcher>): void =>
    setRule((current) => (current.title ? { ...current, title: { ...current.title, ...patch } } : current));

  const setTitleEnabled = (enabled: boolean): void =>
    setRule((current) => {
      if (enabled) return { ...current, title: current.title ?? lastTitle.current };
      if (current.title) lastTitle.current = current.title;
      return { ...current, title: null };
    });

  const pickedTitle = picked ? picked.title.trim() : '';
  const usesPickedTitle =
    pickedTitle !== '' &&
    rule.title !== null &&
    rule.title.mode === 'contains' &&
    rule.title.pattern === pickedTitle;

  const submit = async (): Promise<void> => {
    if (saving) return;
    setSubmitted(true);
    if (!validation.valid) {
      if (validation.errors.exe) exeRef.current?.focus();
      else patternRef.current?.focus();
      return;
    }
    setSaving(true);
    setSaveFailed(false);
    const ok = await onSave(rule);
    if (ok) {
      onClose();
      return;
    }
    setSaving(false);
    setSaveFailed(true);
  };

  return (
    <>
      <Dialog
        title={isNew ? 'New rule' : 'Edit rule'}
        description="Applies to windows as they open, and when you choose Apply rules now."
        onClose={onClose}
        closeDisabled={saving}
        onSubmit={() => void submit()}
        footer={
          <>
            <Button type="submit" variant="accent" busy={saving}>
              {isNew ? 'Create rule' : 'Save rule'}
            </Button>
            <Button onClick={onClose} disabled={saving}>
              Cancel
            </Button>
          </>
        }
      >
        <div className="form-stack">
          <Field label="Name" htmlFor={ids.name} hint="Shown in the rule list. Leave empty to use the program name." hintId={ids.nameHint}>
            <TextInput
              id={ids.name}
              value={rule.name}
              maxLength={1024}
              placeholder="For example: Calculator on top"
              aria-describedby={ids.nameHint}
              onChange={(event) => update({ name: event.currentTarget.value })}
            />
          </Field>

          <Field
            label="Executable"
            required
            htmlFor={ids.exe}
            hint="A file name such as notepad.exe, a full path, or a pattern with * and ?."
            hintId={ids.exeHint}
            error={exeError}
            errorId={ids.exeError}
          >
            <div className="input-with-button">
              <TextInput
                ref={exeRef}
                id={ids.exe}
                value={rule.exe}
                maxLength={1024}
                placeholder="notepad.exe"
                aria-required="true"
                invalid={exeError !== undefined}
                aria-describedby={exeError ? ids.exeError : ids.exeHint}
                onChange={(event) => update({ exe: event.currentTarget.value })}
              />
              <Button icon="window" onClick={() => setPickerOpen(true)}>
                Choose from running windows…
              </Button>
            </div>
          </Field>

          {picked && pickedTitle !== '' && (
            <div className="picked-window">
              <Checkbox
                checked={usesPickedTitle}
                onChange={(checked) =>
                  setRule((current) =>
                    checked ? applyWindowToRule(current, picked, true) : { ...current, title: null },
                  )
                }
                label={<>Also use its title “{displayTitle(picked.title)}” as a title condition</>}
              />
            </div>
          )}

          <div className="form-section" role="group" aria-labelledby={ids.titleSection}>
            <p id={ids.titleSection} className="form-section-title">
              Window title
            </p>
            <Checkbox
              checked={rule.title !== null}
              onChange={setTitleEnabled}
              label="Only match windows with a specific title"
              description="Without a title condition the rule applies to every window of the program."
            />
            {rule.title && (
              <div className="title-condition">
                <div className="field">
                  <label htmlFor={ids.mode} className="field-label">
                    Title
                  </label>
                  <Select
                    id={ids.mode}
                    value={rule.title.mode}
                    options={TITLE_MODE_OPTIONS}
                    onChange={(mode) => updateTitle({ mode })}
                  />
                </div>
                <div className="field title-pattern">
                  <label htmlFor={ids.pattern} className="field-label">
                    Text
                  </label>
                  <TextInput
                    ref={patternRef}
                    id={ids.pattern}
                    value={rule.title.pattern}
                    maxLength={1024}
                    placeholder={rule.title.mode === 'wildcard' ? '*- Notepad' : 'Untitled'}
                    invalid={patternError !== undefined}
                    aria-describedby={patternError ? ids.patternError : undefined}
                    onChange={(event) => updateTitle({ pattern: event.currentTarget.value })}
                  />
                </div>
                <div className="title-case">
                  <Checkbox
                    checked={rule.title.caseSensitive}
                    onChange={(caseSensitive) => updateTitle({ caseSensitive })}
                    label="Case-sensitive"
                  />
                </div>
                {patternError && (
                  <p id={ids.patternError} className="field-error title-error">
                    {patternError}
                  </p>
                )}
              </div>
            )}
          </div>

          <div className="form-section" role="group" aria-labelledby={ids.effectSection}>
            <p id={ids.effectSection} className="form-section-title">
              Effect
            </p>
            <Checkbox
              checked={rule.pin}
              onChange={(pin) => update({ pin })}
              label="Pin matching windows"
              description="Keeps them on top of other windows."
            />
            <div className="form-slider">
              <label htmlFor={ids.opacity} className="field-label">
                Opacity
              </label>
              <Slider id={ids.opacity} value={rule.opacity} onInput={(opacity) => update({ opacity })} />
            </div>
          </div>

          <Checkbox
            checked={rule.enabled}
            onChange={(enabled) => update({ enabled })}
            label="Enable this rule"
          />

          {validation.warnings.map((warning) => (
            <InfoBar key={warning} tone="warning" compact>
              {warning}
            </InfoBar>
          ))}

          {saveFailed && (
            <InfoBar tone="error" role="alert" compact>
              The rule was not saved{saveError ? `: ${saveError}` : '.'}
            </InfoBar>
          )}
        </div>
      </Dialog>

      {pickerOpen && (
        <WindowPicker
          mode="select"
          title="Choose a program"
          description="The rule will match windows of the program you choose."
          onClose={() => setPickerOpen(false)}
          onSelect={(window) => {
            setPicked(window);
            setRule((current) => applyWindowToRule(current, window, false));
          }}
        />
      )}
    </>
  );
}
