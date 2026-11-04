import { useEffect, useEffectEvent, useRef, useState, type CSSProperties } from 'react';
import { cx, formatPercent } from '../lib/format';

interface SliderProps {
  id?: string;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  /** Called continuously while the user drags or presses keys. */
  onInput?: (value: number) => void;
  /**
   * Called once when an interaction ends (pointer release, or each key press). The live
   * value stays visible until a returned promise settles, so the thumb does not jump back
   * while the change is being applied.
   */
  onCommit?: (value: number) => void | Promise<unknown>;
  formatValue?: (value: number) => string;
  showValue?: boolean;
  disabled?: boolean;
  className?: string;
  'aria-label'?: string;
  'aria-labelledby'?: string;
  'aria-describedby'?: string;
}

/** Fluent slider with a value label. */
export function Slider({
  id,
  value,
  min = 0,
  max = 100,
  step = 1,
  onInput,
  onCommit,
  formatValue = formatPercent,
  showValue = true,
  disabled = false,
  className,
  ...aria
}: SliderProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const draftRef = useRef<number | null>(null);
  const [draft, setDraft] = useState<number | null>(null);
  const shown = draft ?? value;
  const fill = max > min ? ((shown - min) / (max - min)) * 100 : 0;

  const clearDraft = (expected: number): void => {
    if (draftRef.current !== expected) return;
    draftRef.current = null;
    setDraft(null);
  };

  // The native `change` event marks the end of an interaction (React's onChange fires on
  // every input event for range inputs).
  const commit = useEffectEvent(() => {
    const committed = draftRef.current;
    if (committed === null) return;
    const result = onCommit?.(committed);
    if (result instanceof Promise) {
      result.then(
        () => clearDraft(committed),
        () => clearDraft(committed),
      );
    } else {
      clearDraft(committed);
    }
  });

  useEffect(() => {
    const input = inputRef.current;
    if (!input) return;
    const handleChange = (): void => commit();
    input.addEventListener('change', handleChange);
    return () => input.removeEventListener('change', handleChange);
  }, []);

  const style = { '--fill': `${fill}%` } as CSSProperties;

  return (
    <div className={cx('slider', className)}>
      <input
        ref={inputRef}
        id={id}
        type="range"
        className="slider-input"
        min={min}
        max={max}
        step={step}
        value={shown}
        disabled={disabled}
        style={style}
        aria-valuetext={formatValue(shown)}
        onChange={(event) => {
          const next = Number(event.currentTarget.value);
          draftRef.current = next;
          setDraft(next);
          onInput?.(next);
        }}
        {...aria}
      />
      {showValue && (
        <span className="slider-value" aria-hidden="true">
          {formatValue(shown)}
        </span>
      )}
    </div>
  );
}
