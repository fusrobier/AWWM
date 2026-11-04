import { useRef, type KeyboardEvent } from 'react';
import { nextIndex, PAGE_TITLES, PAGES } from '../lib/navigation';
import type { Page } from '../lib/types';
import { AppLogo, Icon, type IconName } from './Icon';

const PAGE_ICONS: Record<Page, IconName> = {
  dashboard: 'dashboard',
  rules: 'rules',
  mouse: 'mouse',
  general: 'general',
  about: 'info',
};

interface SidebarProps {
  page: Page;
  onNavigate: (page: Page) => void;
  managementEnabled: boolean;
  /** Pages that need attention (e.g. a settings warning). */
  attention?: ReadonlySet<Page>;
}

/** Left navigation. Arrow keys move focus between items; Enter/Space opens a page. */
export function Sidebar({ page, onNavigate, managementEnabled, attention }: SidebarProps) {
  const listRef = useRef<HTMLUListElement>(null);

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>): void => {
    const items = Array.from(listRef.current?.querySelectorAll<HTMLButtonElement>('.nav-item') ?? []);
    const target = nextIndex(items.indexOf(event.currentTarget), items.length, event.key);
    if (target === null) return;
    event.preventDefault();
    items[target]?.focus();
  };

  return (
    <aside className="sidebar">
      <div className="sidebar-brand">
        <AppLogo size={28} />
        <div className="sidebar-brand-text">
          <span className="sidebar-brand-name">AWWM</span>
          <span className="sidebar-brand-caption">Advanced Window Manager</span>
        </div>
      </div>
      <nav aria-label="Settings sections">
        <ul ref={listRef} className="nav-list">
          {PAGES.map((id) => (
            <li key={id}>
              <button
                type="button"
                className="nav-item"
                aria-current={id === page ? 'page' : undefined}
                onClick={() => onNavigate(id)}
                onKeyDown={onKeyDown}
              >
                <Icon name={PAGE_ICONS[id]} size={16} />
                <span className="nav-label">{PAGE_TITLES[id]}</span>
                {attention?.has(id) && (
                  <>
                    <span className="nav-dot" aria-hidden="true" />
                    <span className="sr-only">(needs attention)</span>
                  </>
                )}
              </button>
            </li>
          ))}
        </ul>
      </nav>
      <div className="sidebar-footer" role="status">
        <span className={managementEnabled ? 'status-dot is-on' : 'status-dot'} aria-hidden="true" />
        {managementEnabled ? 'Window management is on' : 'Window management is paused'}
      </div>
    </aside>
  );
}
