import { useId, useState, type ReactNode } from 'react';
import { cx } from '../lib/format';
import { ColorInput } from './ColorInput';
import { Icon, type IconName } from './Icon';
import { NumberInput } from './NumberInput';
import { Select, type SelectOption } from './Select';
import { Slider } from './Slider';
import { Switch } from './Switch';

interface CardProps {
  title?: ReactNode;
  description?: ReactNode;
  icon?: IconName;
  actions?: ReactNode;
  footer?: ReactNode;
  children?: ReactNode;
  padded?: boolean;
  className?: string;
}

/** A titled group of settings or content. */
export function Card({ title, description, icon, actions, footer, children, padded = false, className }: CardProps) {
  const headingId = useId();
  return (
    <section className={cx('card', className)} aria-labelledby={title ? headingId : undefined}>
      {title && (
        <header className="card-header">
          {icon && <Icon name={icon} className="card-header-icon" />}
          <div className="card-header-text">
            <h2 id={headingId} className="card-title">
              {title}
            </h2>
            {description && <p className="card-description">{description}</p>}
          </div>
          {actions && <div className="card-actions">{actions}</div>}
        </header>
      )}
      {children !== undefined && <div className={cx('card-body', padded && 'card-body--padded')}>{children}</div>}
      {footer && <div className="card-footer">{footer}</div>}
    </section>
  );
}

interface SettingRowProps {
  label: ReactNode;
  description?: ReactNode;
  /** id of the control; the label becomes a <label for>. */
  controlId?: string;
  labelId?: string;
  descriptionId?: string;
  icon?: IconName;
  disabled?: boolean;
  indent?: boolean;
  wideControl?: boolean;
  children?: ReactNode;
}

/** Label + description on the left, control on the right. */
export function SettingRow({
  label,
  description,
  controlId,
  labelId,
  descriptionId,
  icon,
  disabled = false,
  indent = false,
  wideControl = false,
  children,
}: SettingRowProps) {
  return (
    <div className={cx('setting-row', disabled && 'is-disabled', indent && 'setting-row--indent')}>
      {icon && <Icon name={icon} size={20} className="setting-icon" />}
      <div className="setting-text">
        {controlId ? (
          <label id={labelId} htmlFor={controlId} className="setting-label">
            {label}
          </label>
        ) : (
          <span id={labelId} className="setting-label">
            {label}
          </span>
        )}
        {description && (
          <p id={descriptionId} className="setting-description">
            {description}
          </p>
        )}
      </div>
      {children !== undefined && (
        <div className={cx('setting-control', wideControl && 'setting-control--wide')}>{children}</div>
      )}
    </div>
  );
}

interface CommonSettingProps {
  label: ReactNode;
  description?: ReactNode;
  icon?: IconName;
  disabled?: boolean;
  indent?: boolean;
}

export function SwitchSetting({
  checked,
  onChange,
  busy,
  ...row
}: CommonSettingProps & { checked: boolean; onChange: (checked: boolean) => void; busy?: boolean }) {
  const id = useId();
  const descriptionId = `${id}-description`;
  return (
    <SettingRow {...row} controlId={id} descriptionId={descriptionId}>
      <Switch
        id={id}
        checked={checked}
        onChange={onChange}
        disabled={row.disabled}
        busy={busy}
        describedBy={row.description ? descriptionId : undefined}
      />
    </SettingRow>
  );
}

export function SelectSetting<T extends string>({
  value,
  options,
  onChange,
  ...row
}: CommonSettingProps & {
  value: T;
  options: ReadonlyArray<SelectOption<T>>;
  onChange: (value: T) => void;
}) {
  const id = useId();
  const descriptionId = `${id}-description`;
  return (
    <SettingRow {...row} controlId={id} descriptionId={descriptionId}>
      <Select
        id={id}
        value={value}
        options={options}
        onChange={onChange}
        disabled={row.disabled}
        aria-describedby={row.description ? descriptionId : undefined}
      />
    </SettingRow>
  );
}

export function NumberSetting({
  value,
  min,
  max,
  unit,
  onChange,
  ...row
}: CommonSettingProps & {
  value: number;
  min: number;
  max: number;
  unit?: string;
  onChange: (value: number) => void;
}) {
  const id = useId();
  const descriptionId = `${id}-description`;
  return (
    <SettingRow {...row} controlId={id} descriptionId={descriptionId}>
      <NumberInput
        id={id}
        value={value}
        min={min}
        max={max}
        unit={unit}
        onChange={onChange}
        disabled={row.disabled}
        aria-describedby={row.description ? descriptionId : undefined}
      />
    </SettingRow>
  );
}

export function SliderSetting({
  value,
  min,
  max,
  formatValue,
  onCommit,
  footer,
  ...row
}: CommonSettingProps & {
  value: number;
  min: number;
  max: number;
  formatValue?: (value: number) => string;
  onCommit: (value: number) => void;
  /** Rendered under the row (e.g. a warning); receives the live value while dragging. */
  footer?: (liveValue: number) => ReactNode;
}) {
  const id = useId();
  const descriptionId = `${id}-description`;
  const footerId = `${id}-footer`;
  const [live, setLive] = useState<number | null>(null);
  const extra = footer?.(live ?? value);
  const describedBy = [row.description ? descriptionId : null, extra ? footerId : null]
    .filter((part): part is string => part !== null)
    .join(' ');
  return (
    <>
      <SettingRow {...row} controlId={id} descriptionId={descriptionId} wideControl>
        <Slider
          id={id}
          value={value}
          min={min}
          max={max}
          formatValue={formatValue}
          disabled={row.disabled}
          aria-describedby={describedBy || undefined}
          onInput={setLive}
          onCommit={(next) => {
            onCommit(next);
            setLive(null);
          }}
        />
      </SettingRow>
      {extra && (
        <div id={footerId} className="setting-row-footer">
          {extra}
        </div>
      )}
    </>
  );
}

export function ColorSetting({
  value,
  onChange,
  ...row
}: CommonSettingProps & { value: string; onChange: (value: string) => void }) {
  const id = useId();
  const descriptionId = `${id}-description`;
  return (
    <SettingRow {...row} controlId={id} descriptionId={descriptionId}>
      <ColorInput
        id={id}
        value={value}
        onChange={onChange}
        disabled={row.disabled}
        aria-describedby={row.description ? descriptionId : undefined}
      />
    </SettingRow>
  );
}
