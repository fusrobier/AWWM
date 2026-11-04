import type { ButtonHTMLAttributes, MouseEvent, ReactNode, Ref } from 'react';
import { cx } from '../lib/format';
import { Icon, type IconName } from './Icon';

export type ButtonVariant = 'standard' | 'accent' | 'subtle' | 'danger' | 'danger-text' | 'link';

type NativeButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'type'>;

interface ButtonProps extends NativeButtonProps {
  variant?: ButtonVariant;
  size?: 'normal' | 'small';
  icon?: IconName;
  /**
   * Shows a spinner in place of the icon and ignores activation. The button stays
   * focusable (unlike `disabled`), so keyboard focus is not lost while a command runs.
   */
  busy?: boolean;
  type?: 'button' | 'submit';
  children?: ReactNode;
  ref?: Ref<HTMLButtonElement>;
}

function guardBusy(
  busy: boolean,
  onClick: NativeButtonProps['onClick'],
): NativeButtonProps['onClick'] {
  if (!busy) return onClick;
  return (event: MouseEvent<HTMLButtonElement>) => event.preventDefault();
}

export function Button({
  variant = 'standard',
  size = 'normal',
  icon,
  busy = false,
  type = 'button',
  className,
  children,
  onClick,
  ref,
  ...rest
}: ButtonProps) {
  return (
    <button
      ref={ref}
      type={type}
      className={cx(
        'btn',
        variant !== 'standard' && `btn--${variant}`,
        size === 'small' && 'btn--small',
        className,
      )}
      aria-busy={busy || undefined}
      onClick={guardBusy(busy, onClick)}
      {...rest}
    >
      {busy ? <span className="spinner" aria-hidden="true" /> : icon && <Icon name={icon} />}
      {children}
    </button>
  );
}

interface IconButtonProps extends Omit<NativeButtonProps, 'children'> {
  /** Accessible name, also shown as a tooltip. */
  label: string;
  icon: IconName;
  variant?: 'standard' | 'subtle';
  size?: 'normal' | 'small';
  busy?: boolean;
  ref?: Ref<HTMLButtonElement>;
}

export function IconButton({
  label,
  icon,
  variant = 'subtle',
  size = 'normal',
  busy = false,
  className,
  onClick,
  ref,
  ...rest
}: IconButtonProps) {
  return (
    <button
      ref={ref}
      type="button"
      className={cx(
        'btn',
        'btn--icon',
        variant === 'subtle' && 'btn--subtle',
        size === 'small' && 'btn--small',
        className,
      )}
      aria-label={label}
      title={label}
      aria-busy={busy || undefined}
      onClick={guardBusy(busy, onClick)}
      {...rest}
    >
      {busy ? <span className="spinner" aria-hidden="true" /> : <Icon name={icon} />}
    </button>
  );
}
