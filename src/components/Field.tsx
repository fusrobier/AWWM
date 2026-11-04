import type { ReactNode } from 'react';
import { Icon } from './Icon';

interface FieldProps {
  label: ReactNode;
  htmlFor: string;
  hint?: ReactNode;
  hintId?: string;
  error?: string;
  errorId?: string;
  required?: boolean;
  children: ReactNode;
}

/** Stacked form field: label, control, hint and validation message. */
export function Field({ label, htmlFor, hint, hintId, error, errorId, required = false, children }: FieldProps) {
  return (
    <div className="field">
      <label htmlFor={htmlFor} className="field-label">
        {label}
        {required && (
          <span className="field-required" aria-hidden="true">
            {' '}
            *
          </span>
        )}
      </label>
      {children}
      {error ? (
        <p id={errorId} className="field-error">
          <Icon name="error" size={14} />
          {error}
        </p>
      ) : (
        hint && (
          <p id={hintId} className="field-hint">
            {hint}
          </p>
        )
      )}
    </div>
  );
}
