import { useId, useState } from 'react';
import { Icon } from '../components/Icon';
import { Select, type SelectOption } from '../components/Select';
import type { Resource } from '../hooks/useResource';
import { cx } from '../lib/format';
import {
  draftFromHotkey,
  findConflictingAction,
  formatHotkey,
  HOTKEY_ACTION_INFO,
  HOTKEY_MODIFIER_LABELS,
  HOTKEY_MODIFIERS,
  hotkeyFromDraft,
  hotkeysEqual,
  hotkeySignature,
  hotkeyToPersist,
  keyDisplayName,
  toggleModifier,
  validateHotkeyDraft,
  type HotkeyDraft,
} from '../lib/hotkeys';
import type { Hotkey, HotkeyAction, HotkeySettings, HotkeyStatus } from '../lib/types';

const DISABLED_KEY = '';

interface HotkeyEditorProps {
  action: HotkeyAction;
  saved: Hotkey | null;
  status: HotkeyStatus | undefined;
  hotkeys: HotkeySettings;
  keyNames: Resource<string[]>;
  managementEnabled: boolean;
  onSave: (hotkey: Hotkey | null) => Promise<boolean>;
}

/**
 * Modifier chips + key picker for one global shortcut. Valid, conflict-free changes are
 * saved immediately; invalid drafts stay local and explain what is missing.
 */
export function HotkeyEditor({
  action,
  saved,
  status,
  hotkeys,
  keyNames,
  managementEnabled,
  onSave,
}: HotkeyEditorProps) {
  const labelId = useId();
  const modifiersLabelId = useId();
  const keyLabelId = useId();
  const messageId = useId();
  const savedSignature = saved ? hotkeySignature(saved) : 'disabled';
  const [draft, setDraft] = useState<HotkeyDraft>(() => draftFromHotkey(saved));
  const [syncedSignature, setSyncedSignature] = useState(savedSignature);

  // Follow changes of the stored shortcut (save results, resets, other windows).
  if (syncedSignature !== savedSignature) {
    setSyncedSignature(savedSignature);
    setDraft(draftFromHotkey(saved, draft));
  }

  const names = keyNames.status === 'ready' ? keyNames.data : [];
  const invalid = validateHotkeyDraft(draft, names);
  const candidate = invalid === null ? hotkeyFromDraft(draft) : null;
  const conflict = candidate ? findConflictingAction(hotkeys, action, candidate) : null;
  const conflictMessage =
    conflict && candidate
      ? `${formatHotkey(candidate)} is already used for “${HOTKEY_ACTION_INFO[conflict].label}”.`
      : null;
  const error = invalid ?? conflictMessage;

  const change = (next: HotkeyDraft): void => {
    setDraft(next);
    // Invalid or conflicting combinations stay local; only valid changes are saved.
    const decision = hotkeyToPersist(next, names, hotkeys, action);
    if (decision.save) void onSave(decision.hotkey);
  };

  const keyOptions: Array<SelectOption<string>> = [{ value: DISABLED_KEY, label: 'Disabled' }];
  for (const name of names) keyOptions.push({ value: name, label: keyDisplayName(name) });
  if (draft.key !== null && !names.some((name) => name.toLowerCase() === draft.key?.toLowerCase())) {
    keyOptions.push({ value: draft.key, label: keyDisplayName(draft.key) });
  }

  let message = null;
  if (error) {
    message = (
      <span className="hotkey-message is-error">
        <Icon name="error" size={14} />
        {error}
      </span>
    );
  } else if (saved === null) {
    message = <span className="hotkey-message">Disabled</span>;
  } else if (status && hotkeysEqual(saved, candidate)) {
    if (status.registered) {
      message = (
        <span className="hotkey-message is-ok">
          <Icon name="check" size={14} />
          {status.label} is active
        </span>
      );
    } else if (status.error) {
      message = (
        <span className="hotkey-message is-error">
          <Icon name="error" size={14} />
          {status.label} could not be registered: {status.error}
        </span>
      );
    } else {
      message = (
        <span className="hotkey-message">
          {managementEnabled ? `${status.label} is not active` : `${status.label} is inactive while window management is paused`}
        </span>
      );
    }
  }

  return (
    <div className="hotkey-row">
      <div className="hotkey-text">
        <span id={labelId} className="setting-label">
          {HOTKEY_ACTION_INFO[action].label}
        </span>
        <p id={messageId} className="hotkey-status" role="status">
          {message}
        </p>
      </div>
      <div className="hotkey-controls">
        <span id={modifiersLabelId} className="sr-only">
          modifier keys
        </span>
        <div className="chip-group" role="group" aria-labelledby={`${labelId} ${modifiersLabelId}`}>
          {HOTKEY_MODIFIERS.map((modifier) => (
            <button
              key={modifier}
              type="button"
              className="chip"
              aria-pressed={draft[modifier]}
              aria-describedby={error ? messageId : undefined}
              disabled={draft.key === null}
              onClick={() => change(toggleModifier(draft, modifier))}
            >
              {HOTKEY_MODIFIER_LABELS[modifier]}
            </button>
          ))}
        </div>
        <span className={cx('hotkey-plus', draft.key === null && 'is-muted')} aria-hidden="true">
          +
        </span>
        <span id={keyLabelId} className="sr-only">
          key
        </span>
        <Select
          className="hotkey-key"
          value={draft.key ?? DISABLED_KEY}
          options={keyNames.status === 'ready' ? keyOptions : [{ value: draft.key ?? DISABLED_KEY, label: draft.key ? keyDisplayName(draft.key) : 'Loading keys…' }]}
          disabled={keyNames.status !== 'ready'}
          aria-labelledby={`${labelId} ${keyLabelId}`}
          aria-describedby={error ? messageId : undefined}
          onChange={(value) => change({ ...draft, key: value === DISABLED_KEY ? null : value })}
        />
      </div>
    </div>
  );
}
