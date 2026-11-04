import { useState } from 'react';
import { isHexColor } from '../lib/format';

interface ColorInputProps {
  id: string;
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  'aria-describedby'?: string;
}

/** Colour swatch (native picker) plus a #RRGGBB text field. */
export function ColorInput({ id, value, onChange, disabled = false, 'aria-describedby': describedBy }: ColorInputProps) {
  const [text, setText] = useState<string | null>(null);
  const shown = text ?? value.toUpperCase();
  const invalid = text !== null && !isHexColor(text.trim());
  const hexId = `${id}-hex`;

  return (
    <span className="color-field">
      <input
        id={id}
        type="color"
        className="color-swatch"
        value={isHexColor(value) ? value.toLowerCase() : '#000000'}
        disabled={disabled}
        aria-describedby={describedBy}
        onChange={(event) => onChange(event.currentTarget.value.toUpperCase())}
      />
      <label htmlFor={hexId} className="sr-only">
        Hex colour code
      </label>
      <input
        id={hexId}
        type="text"
        className="text-input"
        value={shown}
        maxLength={7}
        spellCheck={false}
        autoComplete="off"
        disabled={disabled}
        aria-invalid={invalid || undefined}
        onFocus={() => setText(value.toUpperCase())}
        onChange={(event) => {
          const raw = event.currentTarget.value;
          setText(raw);
          const candidate = raw.trim().startsWith('#') ? raw.trim() : `#${raw.trim()}`;
          if (isHexColor(candidate) && candidate.toUpperCase() !== value.toUpperCase()) {
            onChange(candidate.toUpperCase());
          }
        }}
        onBlur={() => setText(null)}
      />
    </span>
  );
}
