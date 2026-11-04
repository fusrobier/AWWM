import { useState } from 'react';
import { interpretIntegerInput } from '../lib/numbers';

interface NumberInputProps {
  id?: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  /** Called with every valid in-range value while typing, and with the clamped value on blur. */
  onChange: (value: number) => void;
  unit?: string;
  disabled?: boolean;
  'aria-describedby'?: string;
  'aria-label'?: string;
}

/**
 * Integer field with clamping. While focused the raw text is kept locally; out-of-range
 * or invalid text is flagged and corrected (clamped or reverted) when the field loses
 * focus or Enter is pressed.
 */
export function NumberInput({
  id,
  value,
  min,
  max,
  step = 1,
  onChange,
  unit,
  disabled = false,
  'aria-describedby': describedBy,
  'aria-label': ariaLabel,
}: NumberInputProps) {
  const [text, setText] = useState<string | null>(null);
  const shown = text ?? String(value);
  const parsed = interpretIntegerInput(shown, min, max);
  const invalid = text !== null && parsed.kind !== 'valid';
  const hintId = id ? `${id}-range` : undefined;

  const commit = (): number => {
    let next = value;
    if (parsed.kind === 'valid') next = parsed.value;
    else if (parsed.kind === 'outOfRange') next = parsed.clamped;
    if (next !== value) onChange(next);
    return next;
  };

  return (
    <span className="number-field">
      <input
        id={id}
        className="text-input"
        type="number"
        inputMode="numeric"
        min={min}
        max={max}
        step={step}
        value={shown}
        disabled={disabled}
        aria-invalid={invalid || undefined}
        aria-label={ariaLabel}
        aria-describedby={[describedBy, invalid ? hintId : undefined].filter(Boolean).join(' ') || undefined}
        onFocus={() => setText(String(value))}
        onChange={(event) => {
          const raw = event.currentTarget.value;
          setText(raw);
          const result = interpretIntegerInput(raw, min, max);
          if (result.kind === 'valid' && result.value !== value) onChange(result.value);
        }}
        onBlur={() => {
          commit();
          setText(null);
        }}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault();
            setText(String(commit()));
          } else if (event.key === 'Escape' && text !== null) {
            // Revert to the stored value instead of closing a surrounding dialog.
            event.preventDefault();
            event.stopPropagation();
            setText(String(value));
          }
        }}
      />
      {unit && <span className="number-unit">{unit}</span>}
      {invalid && hintId && (
        <span id={hintId} className="sr-only">
          Enter a whole number from {min} to {max}.
        </span>
      )}
    </span>
  );
}
