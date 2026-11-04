import { cx } from '../lib/format';

interface SwitchProps {
  id?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  /** Accessible name when no <label for> points at the switch. */
  label?: string;
  describedBy?: string;
  /** Shows "On"/"Off" next to the switch, like Windows Settings. */
  showState?: boolean;
  busy?: boolean;
}

/** Fluent toggle switch: a button with role="switch". */
export function Switch({
  id,
  checked,
  onChange,
  disabled = false,
  label,
  describedBy,
  showState = true,
  busy = false,
}: SwitchProps) {
  return (
    <span className={cx('switch-field', disabled && 'is-disabled')}>
      {showState && (
        <span className="switch-state" aria-hidden="true">
          {checked ? 'On' : 'Off'}
        </span>
      )}
      <button
        id={id}
        type="button"
        role="switch"
        className="switch"
        aria-checked={checked}
        aria-label={label}
        aria-describedby={describedBy}
        aria-busy={busy || undefined}
        disabled={disabled}
        onClick={() => {
          // While busy the switch keeps focus but ignores activation.
          if (!busy) onChange(!checked);
        }}
      />
    </span>
  );
}
