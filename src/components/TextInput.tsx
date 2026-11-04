import type { InputHTMLAttributes, Ref } from 'react';
import { cx } from '../lib/format';

interface TextInputProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type'> {
  type?: 'text' | 'search';
  invalid?: boolean;
  ref?: Ref<HTMLInputElement>;
}

export function TextInput({ type = 'text', invalid = false, className, ref, ...rest }: TextInputProps) {
  return (
    <input
      ref={ref}
      type={type}
      className={cx('text-input', className)}
      spellCheck={false}
      autoComplete="off"
      aria-invalid={invalid || undefined}
      {...rest}
    />
  );
}
