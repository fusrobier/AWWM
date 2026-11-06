import { useSyncExternalStore } from 'react';

// A `(resolution: Ndppx)` query only reports leaving the current ratio, so it is replaced
// by a query for the new ratio after every change (display scaling, zoom, other monitor).
function subscribe(onChange: () => void): () => void {
  let media: MediaQueryList | null = null;
  const listen = (): void => {
    media?.removeEventListener('change', handleChange);
    media = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
    media.addEventListener('change', handleChange);
  };
  function handleChange(): void {
    listen();
    onChange();
  }
  listen();
  return () => media?.removeEventListener('change', handleChange);
}

const getSnapshot = (): number => window.devicePixelRatio || 1;
const getServerSnapshot = (): number => 1;

/** Device pixels per CSS pixel; updates when the display scaling or zoom changes. */
export function useDevicePixelRatio(): number {
  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}
