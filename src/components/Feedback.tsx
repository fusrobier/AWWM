import { useEffect, useState, type ReactNode } from 'react';
import { cx } from '../lib/format';
import type { SaveStatus } from '../lib/saveQueue';
import { IconButton } from './Button';
import { Icon, type IconName } from './Icon';

type InfoBarTone = 'info' | 'warning' | 'error' | 'success';

const TONE_ICONS: Record<InfoBarTone, IconName> = {
  info: 'info',
  warning: 'warning',
  error: 'error',
  success: 'success',
};

interface InfoBarProps {
  tone?: InfoBarTone;
  title?: ReactNode;
  children?: ReactNode;
  actions?: ReactNode;
  onDismiss?: () => void;
  compact?: boolean;
  id?: string;
  /** Announce the message when it appears (for validation errors). */
  role?: 'alert' | 'status';
}

/** Inline message bar (Fluent InfoBar). */
export function InfoBar({ tone = 'info', title, children, actions, onDismiss, compact = false, id, role }: InfoBarProps) {
  return (
    <div id={id} role={role} className={cx('infobar', `infobar--${tone}`, compact && 'infobar--compact')}>
      <Icon name={TONE_ICONS[tone]} className="infobar-icon" />
      <div className="infobar-content">
        {title && <p className="infobar-title">{title}</p>}
        {children && <div className="infobar-message">{children}</div>}
      </div>
      {actions && <div className="infobar-actions">{actions}</div>}
      {onDismiss && <IconButton label="Dismiss" icon="close" size="small" onClick={onDismiss} />}
    </div>
  );
}

export type BadgeTone = 'neutral' | 'accent' | 'warning' | 'success' | 'danger';

export function Badge({
  tone = 'neutral',
  icon,
  children,
  title,
}: {
  tone?: BadgeTone;
  icon?: IconName;
  children: ReactNode;
  title?: string;
}) {
  return (
    <span className={cx('badge', tone !== 'neutral' && `badge--${tone}`)} title={title}>
      {icon && <Icon name={icon} size={12} />}
      {children}
    </span>
  );
}

interface EmptyStateProps {
  icon: IconName;
  title: string;
  children?: ReactNode;
  actions?: ReactNode;
}

export function EmptyState({ icon, title, children, actions }: EmptyStateProps) {
  return (
    <div className="empty-state">
      <span className="empty-state-icon">
        <Icon name={icon} size={24} />
      </span>
      <p className="empty-state-title">{title}</p>
      {children && <div className="empty-state-text">{children}</div>}
      {actions && <div className="empty-state-actions">{actions}</div>}
    </div>
  );
}

/** Subtle "Saving… / Saved / Not saved" indicator for the settings pipeline. */
export function SaveIndicator({ status, error }: { status: SaveStatus; error: string | null }) {
  const [showSaved, setShowSaved] = useState(false);

  useEffect(() => {
    if (status !== 'saved') return undefined;
    setShowSaved(true);
    const timer = setTimeout(() => setShowSaved(false), 2500);
    return () => clearTimeout(timer);
  }, [status]);

  let content: ReactNode = null;
  if (status === 'pending' || status === 'saving') {
    content = (
      <>
        <span className="spinner" aria-hidden="true" />
        Saving…
      </>
    );
  } else if (status === 'error') {
    content = (
      <>
        <Icon name="error" size={12} />
        Not saved
      </>
    );
  } else if (status === 'saved' && showSaved) {
    content = (
      <>
        <Icon name="check" size={12} />
        Saved
      </>
    );
  }

  return (
    <span
      className={cx('save-indicator', status === 'saved' && 'is-saved', status === 'error' && 'is-error')}
      role="status"
      title={status === 'error' && error ? error : undefined}
    >
      {content}
    </span>
  );
}

interface PageHeaderProps {
  title: string;
  description?: ReactNode;
  actions?: ReactNode;
  status?: ReactNode;
}

export function PageHeader({ title, description, actions, status }: PageHeaderProps) {
  return (
    <header className="page-header">
      <div className="page-header-text">
        <h1 id="page-title" className="page-title">
          {title}
        </h1>
        {description && <p className="page-description">{description}</p>}
      </div>
      {(actions || status) && (
        <div className="page-header-side">
          {status}
          {actions}
        </div>
      )}
    </header>
  );
}
