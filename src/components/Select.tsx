import { cx } from '../lib/format';
import { Icon } from './Icon';

export interface SelectOption<T extends string> {
  value: T;
  label: string;
  disabled?: boolean;
}

interface SelectProps<T extends string> {
  id?: string;
  value: T;
  options: ReadonlyArray<SelectOption<T>>;
  onChange: (value: T) => void;
  disabled?: boolean;
  className?: string;
  'aria-label'?: string;
  'aria-labelledby'?: string;
  'aria-describedby'?: string;
}

/** Native <select> styled as a Fluent ComboBox. */
export function Select<T extends string>({
  id,
  value,
  options,
  onChange,
  disabled = false,
  className,
  ...aria
}: SelectProps<T>) {
  return (
    <span className={cx('select', className)}>
      <select
        id={id}
        className="select-input"
        value={value}
        disabled={disabled}
        onChange={(event) => {
          const selected = options.find((option) => option.value === event.currentTarget.value);
          if (selected) onChange(selected.value);
        }}
        {...aria}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value} disabled={option.disabled}>
            {option.label}
          </option>
        ))}
      </select>
      <Icon name="chevronDown" size={12} className="select-chevron" />
    </span>
  );
}
