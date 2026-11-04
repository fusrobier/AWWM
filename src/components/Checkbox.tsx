import { useId, type ReactNode } from 'react';
import { cx } from '../lib/format';

interface CheckboxProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: ReactNode;
  description?: ReactNode;
  disabled?: boolean;
  id?: string;
}

export function Checkbox({ checked, onChange, label, description, disabled = false, id }: CheckboxProps) {
  const generated = useId();
  const inputId = id ?? generated;
  const descriptionId = `${inputId}-description`;
  return (
    <label className={cx('check', disabled && 'is-disabled')} htmlFor={inputId}>
      <input
        id={inputId}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        aria-describedby={description ? descriptionId : undefined}
        onChange={(event) => onChange(event.currentTarget.checked)}
      />
      <span className="check-text">
        <span>{label}</span>
        {description && (
          <span id={descriptionId} className="check-description">
            {description}
          </span>
        )}
      </span>
    </label>
  );
}

interface RadioOption<T extends string> {
  value: T;
  label: string;
  description?: string;
}

interface RadioGroupProps<T extends string> {
  name: string;
  value: T;
  options: ReadonlyArray<RadioOption<T>>;
  onChange: (value: T) => void;
  disabled?: boolean;
  'aria-labelledby'?: string;
}

export function RadioGroup<T extends string>({
  name,
  value,
  options,
  onChange,
  disabled = false,
  'aria-labelledby': labelledBy,
}: RadioGroupProps<T>) {
  const baseId = useId();
  return (
    <div className="radio-group" role="radiogroup" aria-labelledby={labelledBy}>
      {options.map((option) => {
        const optionId = `${baseId}-${option.value}`;
        return (
          <label key={option.value} className={cx('check', disabled && 'is-disabled')} htmlFor={optionId}>
            <input
              id={optionId}
              type="radio"
              name={name}
              value={option.value}
              checked={value === option.value}
              disabled={disabled}
              onChange={() => onChange(option.value)}
            />
            <span className="check-text">
              <span>{option.label}</span>
              {option.description && <span className="check-description">{option.description}</span>}
            </span>
          </label>
        );
      })}
    </div>
  );
}
