import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { Button } from '../components/Button';
import { Badge } from '../components/Feedback';
import { Icon } from '../components/Icon';
import { Slider } from '../components/Slider';
import { WindowIcon } from '../components/WindowIcon';
import { useCommandRunner } from '../hooks/useCommand';
import { api } from '../lib/api';
import { isNearlyInvisible, NEARLY_INVISIBLE_WARNING } from '../lib/opacity';
import { createLatestSerial } from '../lib/serial';
import { throttle } from '../lib/timing';
import type { ManagedWindow, Rule } from '../lib/types';
import {
  applyManagedResult,
  displayTitle,
  isReleased,
  managedSourceLabel,
  managedSourceTone,
  removeManaged,
} from '../lib/windows';
import { useAppState } from '../state/appStateContext';
import { useToasts } from '../state/toastContext';

/** Interval for live opacity updates while the slider is dragged. */
const LIVE_OPACITY_INTERVAL_MS = 40;

interface ManagedWindowRowProps {
  window: ManagedWindow;
  rules: readonly Rule[];
}

export function ManagedWindowRow({ window, rules }: ManagedWindowRowProps) {
  const { updateManaged } = useAppState();
  const { notify, notifyError } = useToasts();
  const { run, isBusy } = useCommandRunner();
  const [liveOpacity, setLiveOpacity] = useState<number | null>(null);
  const dragErrorShown = useRef(false);
  const rowRef = useRef<HTMLLIElement>(null);
  const titleId = useId();
  const sliderId = useId();
  const sliderLabelId = useId();
  const warningId = useId();
  const { hwnd } = window;
  const title = displayTitle(window.title);

  /** Keeps keyboard focus in the list when this row disappears. */
  const moveFocusAway = (): void => {
    const row = rowRef.current;
    if (!row || !row.contains(document.activeElement)) return;
    const neighbour = row.nextElementSibling ?? row.previousElementSibling;
    const target = neighbour?.querySelector<HTMLElement>('button');
    if (target) target.focus();
    else document.getElementById('main-content')?.focus();
  };

  // All opacity requests of this row go through one ordered chain, so the final value
  // can never be overtaken by an earlier live update. Live updates are throttled to one
  // per 40 ms and skipped when a newer value is already queued.
  const opacityRequests = useMemo(
    () => createLatestSerial((value: number) => api.setWindowOpacity(hwnd, value)),
    [hwnd],
  );

  // Only the first failure of a drag is reported, so a vanished window does not flood
  // the screen with toasts.
  const sendLiveOpacity = useMemo(
    () =>
      throttle((value: number) => {
        opacityRequests.offer(value).catch((error: unknown) => {
          if (dragErrorShown.current) return;
          dragErrorShown.current = true;
          notifyError(error, 'Opacity could not be changed');
        });
      }, LIVE_OPACITY_INTERVAL_MS),
    [opacityRequests, notifyError],
  );

  useEffect(() => () => sendLiveOpacity.cancel(), [sendLiveOpacity]);

  const commitOpacity = async (value: number): Promise<void> => {
    sendLiveOpacity.cancel();
    try {
      const updated = await opacityRequests.commit(value);
      if (isReleased(updated)) {
        // Not pinned and fully opaque: the backend released the window.
        moveFocusAway();
        notify('success', `“${title}” is fully opaque and not pinned, so AWWM no longer manages it.`);
      }
      updateManaged((list) => applyManagedResult(list, updated));
    } catch (error) {
      if (!dragErrorShown.current) notifyError(error, 'Opacity could not be changed');
    } finally {
      dragErrorShown.current = false;
      setLiveOpacity(null);
    }
  };

  const bringToFront = (): void => {
    void run('activate', () => api.activateWindow(hwnd), 'The window could not be brought to the front');
  };

  const pin = async (): Promise<void> => {
    const result = await run('pin', () => api.pinWindow(hwnd), 'The window was not pinned');
    if (result.ok) updateManaged((list) => applyManagedResult(list, result.value));
  };

  const unpin = async (): Promise<void> => {
    const result = await run('unpin', () => api.unpinWindow(hwnd), 'The window was not released');
    if (!result.ok) return;
    moveFocusAway();
    updateManaged((list) => removeManaged(list, hwnd));
    notify('success', `Released “${title}”: always-on-top and opacity are restored.`);
  };

  const shownOpacity = liveOpacity ?? window.opacity;
  const nearlyInvisible = isNearlyInvisible(shownOpacity);

  return (
    <li ref={rowRef} className="managed-row" aria-labelledby={titleId}>
      <WindowIcon exePath={window.exePath} size={32} />
      <div className="managed-main">
        <p id={titleId} className="managed-title" title={window.title}>
          {title}
        </p>
        <p className="managed-meta">
          <span className="managed-exe" title={window.exePath}>
            {window.exeName}
          </span>
          <Badge tone={managedSourceTone(window.source)}>{managedSourceLabel(window, rules)}</Badge>
          {window.pinned && window.pinPending ? (
            <span title="Windows does not allow this window to stay on top right now (usually a full-screen app has the foreground). AWWM pins it automatically as soon as that changes.">
              <Badge icon="pin">Waiting to pin</Badge>
            </span>
          ) : window.pinned ? (
            <Badge tone="accent" icon="pin">
              Pinned
            </Badge>
          ) : (
            <Badge icon="opacity">Not pinned</Badge>
          )}
        </p>
      </div>
      <div className="managed-actions">
        <Button
          size="small"
          icon="activate"
          busy={isBusy('activate')}
          aria-describedby={titleId}
          onClick={bringToFront}
        >
          Bring to front
        </Button>
        {(!window.pinned || window.pinPending) && (
          <Button size="small" icon="pin" busy={isBusy('pin')} aria-describedby={titleId} onClick={() => void pin()}>
            {window.pinPending ? 'Pin now' : 'Pin'}
          </Button>
        )}
        <Button
          size="small"
          icon="unpin"
          busy={isBusy('unpin')}
          aria-describedby={titleId}
          title="Release the window: restores its normal Z-order and full opacity"
          onClick={() => void unpin()}
        >
          Unpin
        </Button>
      </div>
      <div className="managed-opacity">
        <span id={sliderLabelId} className="managed-opacity-label">
          Opacity
        </span>
        <Slider
          id={sliderId}
          value={window.opacity}
          aria-labelledby={`${sliderLabelId} ${titleId}`}
          aria-describedby={nearlyInvisible ? warningId : undefined}
          onInput={(value) => {
            setLiveOpacity(value);
            sendLiveOpacity(value);
          }}
          onCommit={commitOpacity}
        />
      </div>
      {nearlyInvisible && (
        <p id={warningId} className="managed-warning">
          <Icon name="warning" size={14} />
          {NEARLY_INVISIBLE_WARNING}
        </p>
      )}
    </li>
  );
}
