// Inline stroke icons drawn on a 16×16 grid (no icon package).
import type { ReactNode } from 'react';
import { cx } from '../lib/format';

const dot = (cx: number, cy: number): ReactNode => (
  <circle cx={cx} cy={cy} r="0.8" fill="currentColor" stroke="none" />
);

const ICONS = {
  dashboard: (
    <>
      <rect x="2" y="2" width="5" height="5" rx="1" />
      <rect x="9" y="2" width="5" height="5" rx="1" />
      <rect x="2" y="9" width="5" height="5" rx="1" />
      <rect x="9" y="9" width="5" height="5" rx="1" />
    </>
  ),
  rules: <path d="M9 1.5 3.5 9H8l-1 5.5L12.5 7H8z" />,
  mouse: (
    <>
      <rect x="4" y="1.5" width="8" height="13" rx="4" />
      <path d="M8 4v2.5" />
    </>
  ),
  general: (
    <>
      <path d="M2 4.5h6.5M12.5 4.5H14M2 11.5h1.5M7.5 11.5H14" />
      <circle cx="10.5" cy="4.5" r="2" />
      <circle cx="5.5" cy="11.5" r="2" />
    </>
  ),
  info: (
    <>
      <circle cx="8" cy="8" r="6.5" />
      <path d="M8 7.25v4" />
      {dot(8, 4.9)}
    </>
  ),
  pin: <path d="M5.5 2h5M6.5 2v3.8L4.5 9h7L9.5 5.8V2M8 9v5" />,
  unpin: <path d="M5.5 2h5M6.5 2v3.8L4.5 9h7L9.5 5.8V2M8 9v5M2 2.5l12 11" />,
  window: (
    <>
      <rect x="1.5" y="2.5" width="13" height="11" rx="1.5" />
      <path d="M1.5 5.5h13" />
    </>
  ),
  plus: <path d="M8 3v10M3 8h10" />,
  edit: <path d="M10.5 2.5l3 3L6 13H3v-3zM9 4l3 3" />,
  trash: <path d="M2.5 4h11M6 4V2.5h4V4M3.8 4l.7 9.5h7l.7-9.5M6.6 6.5v4.5M9.4 6.5v4.5" />,
  arrowUp: <path d="M8 13V3M4 7l4-4 4 4" />,
  arrowDown: <path d="M8 3v10M4 9l4 4 4-4" />,
  chevronDown: <path d="M4 6l4 4 4-4" />,
  search: (
    <>
      <circle cx="7" cy="7" r="4.5" />
      <path d="M10.5 10.5 14 14" />
    </>
  ),
  refresh: <path d="M13.5 8a5.5 5.5 0 1 1-1-3.2M12.8 2v3h-3" />,
  check: <path d="M3 8.5l3 3 7-7" />,
  success: (
    <>
      <circle cx="8" cy="8" r="6.5" />
      <path d="M5 8.2l2 2 4-4" />
    </>
  ),
  warning: (
    <>
      <path d="M8 2 14.5 13.5h-13z" />
      <path d="M8 6.5v3" />
      {dot(8, 11.4)}
    </>
  ),
  error: (
    <>
      <circle cx="8" cy="8" r="6.5" />
      <path d="M5.8 5.8l4.4 4.4M10.2 5.8l-4.4 4.4" />
    </>
  ),
  close: <path d="M4 4l8 8M12 4l-8 8" />,
  folder: (
    <path d="M1.5 4.5a1 1 0 0 1 1-1h3.6l1.5 1.5h5.9a1 1 0 0 1 1 1v6.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z" />
  ),
  document: <path d="M3.5 1.5h6l3 3v10h-9zM9.5 1.5v3h3M5.5 8h5M5.5 10.5h5" />,
  play: <path d="M5 3.5v9l7-4.5z" />,
  keyboard: (
    <>
      <rect x="1.5" y="4" width="13" height="8" rx="1.5" />
      <path d="M4 6.5h.01M6.5 6.5h.01M9 6.5h.01M11.5 6.5h.01M5 9.5h6" />
    </>
  ),
  shield: <path d="M8 1.5 13.5 3.5v4c0 3.3-2.3 5.8-5.5 7-3.2-1.2-5.5-3.7-5.5-7v-4z" />,
  activate: (
    <>
      <rect x="1.5" y="5.5" width="9" height="9" rx="1.5" />
      <path d="M9 1.5h5.5V7M14.5 1.5 8.5 7.5" />
    </>
  ),
  power: <path d="M8 1.5v6M4.6 3.6a5.5 5.5 0 1 0 6.8 0" />,
  external: (
    <path d="M9 2.5h4.5V7M13.5 2.5 7.5 8.5M11.5 9.5v3a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1v-7a1 1 0 0 1 1-1h3" />
  ),
  opacity: <path d="M8 1.8c2.7 3 4.3 5.3 4.3 7.3a4.3 4.3 0 0 1-8.6 0c0-2 1.6-4.3 4.3-7.3z" />,
  snap: (
    <>
      <rect x="1.5" y="1.5" width="13" height="13" rx="1.5" />
      <path d="M8 1.5v13M1.5 8h13" />
    </>
  ),
  ban: (
    <>
      <circle cx="8" cy="8" r="6.5" />
      <path d="M3.4 3.4l9.2 9.2" />
    </>
  ),
  frame: (
    <>
      <rect x="1.5" y="1.5" width="13" height="13" rx="1.5" />
      <rect x="4" y="4" width="8" height="8" rx="0.5" />
    </>
  ),
  restore: <path d="M2.5 8a5.5 5.5 0 1 0 1.6-3.9M2.5 2v3h3" />,
  startup: (
    <>
      <rect x="1.5" y="9.5" width="13" height="4" rx="1" />
      <path d="M8 7.5V1.5M5 4.5l3-3 3 3" />
    </>
  ),
  palette: (
    <>
      <path d="M8 1.5a6.5 6.5 0 1 0 0 13c1 0 1.5-.6 1.5-1.4 0-.9-.8-1.3-.8-2.1 0-.8.7-1.5 1.5-1.5h1.6a2.7 2.7 0 0 0 2.7-2.7C14.5 4 11.6 1.5 8 1.5z" />
      {dot(5, 6.5)}
      {dot(8, 4.5)}
      {dot(11, 6.5)}
    </>
  ),
  bug: (
    <>
      <rect x="5" y="4.5" width="6" height="9" rx="3" />
      <path d="M8 7.5v6M2.5 9h2.5M11 9h2.5M3 5l2 1.5M13 5l-2 1.5M3 13l2-1.5M13 13l-2-1.5M6.2 4.5 5.5 2.5M9.8 4.5l.7-2" />
    </>
  ),
  heart: <path d="M8 13.5S2 10 2 6a3 3 0 0 1 6-1 3 3 0 0 1 6 1c0 4-6 7.5-6 7.5z" />,
} satisfies Record<string, ReactNode>;

export type IconName = keyof typeof ICONS;

interface IconProps {
  name: IconName;
  size?: number;
  className?: string;
}

export function Icon({ name, size = 16, className }: IconProps) {
  return (
    <svg
      className={cx('icon', className)}
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.25}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {ICONS[name]}
    </svg>
  );
}

/** The AWWM product mark: two stacked windows on an accent tile. */
export function AppLogo({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true" focusable="false" className="icon">
      <rect width="32" height="32" rx="8" fill="var(--brand)" />
      <rect x="6.5" y="8" width="13" height="11" rx="2" fill="#ffffff" fillOpacity="0.5" />
      <rect x="12.5" y="13" width="13" height="11" rx="2" fill="#ffffff" />
      <rect x="12.5" y="13" width="13" height="3" rx="1.5" fill="#bfdbfe" />
    </svg>
  );
}
