import { useState } from 'react';
import { useWindowIcon } from '../hooks/useWindowIcon';
import { Icon } from './Icon';

interface WindowIconProps {
  exePath: string;
  size?: number;
}

/** The executable's icon, or a generic window glyph while loading / when unavailable. */
export function WindowIcon({ exePath, size = 24 }: WindowIconProps) {
  const icon = useWindowIcon(exePath);
  const [failedSrc, setFailedSrc] = useState<string | null>(null);

  if (icon && icon !== failedSrc) {
    return (
      <img
        className="window-icon"
        src={icon}
        width={size}
        height={size}
        alt=""
        draggable={false}
        onError={() => setFailedSrc(icon)}
      />
    );
  }

  return (
    <span className="window-icon window-icon--fallback" style={{ width: size, height: size }} aria-hidden="true">
      <Icon name="window" size={Math.round(size * 0.75)} />
    </span>
  );
}
