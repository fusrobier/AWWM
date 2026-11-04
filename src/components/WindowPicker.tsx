import { useCallback, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { api } from '../lib/api';
import { errorMessage } from '../lib/backend';
import { cx, pluralize } from '../lib/format';
import { nextIndex } from '../lib/navigation';
import type { WindowInfo } from '../lib/types';
import { applyManagedResult, ELEVATED_WINDOW_NOTE, displayTitle, filterWindows } from '../lib/windows';
import { useAppState } from '../state/appStateContext';
import { useToasts } from '../state/toastContext';
import { Button } from './Button';
import { Dialog } from './Dialog';
import { Badge, InfoBar } from './Feedback';
import { Icon } from './Icon';
import { WindowIcon } from './WindowIcon';

type PickerProps =
  | { mode: 'pin'; onClose: () => void }
  | {
      mode: 'select';
      onClose: () => void;
      /** Receives the chosen window; the picker closes itself afterwards. */
      onSelect: (window: WindowInfo) => void;
      title?: string;
      description?: string;
    };

/**
 * Lists the current top-level windows. In "pin" mode choosing a window pins it; in
 * "select" mode the chosen window is returned to the caller.
 */
export function WindowPicker(props: PickerProps) {
  const { snapshot, updateManaged } = useAppState();
  const { notify, notifyError } = useToasts();
  const [windows, setWindows] = useState<WindowInfo[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [pinning, setPinning] = useState<number | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const requestId = useRef(0);
  const searchId = useId();
  const baseId = useId();

  const refresh = useCallback(async () => {
    requestId.current += 1;
    const request = requestId.current;
    setLoading(true);
    setLoadError(null);
    try {
      const result = await api.listWindows();
      if (request === requestId.current) setWindows(result);
    } catch (error) {
      if (request === requestId.current) setLoadError(errorMessage(error));
    } finally {
      if (request === requestId.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const managedByHwnd = useMemo(
    () => new Map(snapshot.managed.map((window) => [window.hwnd, window])),
    [snapshot.managed],
  );

  const visible = useMemo(() => filterWindows(windows ?? [], query), [windows, query]);

  const unavailableReason = (window: WindowInfo): string | null => {
    if (!window.accessible) return ELEVATED_WINDOW_NOTE;
    if (props.mode === 'pin' && managedByHwnd.get(window.hwnd)?.pinned) return 'Already pinned';
    return null;
  };

  const choose = async (window: WindowInfo): Promise<void> => {
    if (unavailableReason(window) !== null || pinning !== null) return;
    if (props.mode === 'select') {
      props.onSelect(window);
      props.onClose();
      return;
    }
    setPinning(window.hwnd);
    setActionError(null);
    try {
      const managed = await api.pinWindow(window.hwnd);
      updateManaged((list) => applyManagedResult(list, managed));
      notify('success', `Pinned “${displayTitle(managed.title)}”.`);
      props.onClose();
    } catch (error) {
      setActionError(errorMessage(error));
      notifyError(error, 'The window was not pinned');
      setPinning(null);
    }
  };

  const rows = (): HTMLButtonElement[] =>
    Array.from(listRef.current?.querySelectorAll<HTMLButtonElement>('[data-picker-row]') ?? []);

  const onSearchKeyDown = (event: KeyboardEvent<HTMLInputElement>): void => {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      rows()[0]?.focus();
    } else if (event.key === 'Enter') {
      event.preventDefault();
      const first = visible.find((window) => unavailableReason(window) === null);
      if (first && visible.length === 1) void choose(first);
    }
  };

  const onRowKeyDown = (event: KeyboardEvent<HTMLButtonElement>): void => {
    const items = rows();
    const index = items.indexOf(event.currentTarget);
    if (event.key === 'ArrowUp' && index === 0) {
      event.preventDefault();
      searchRef.current?.focus();
      return;
    }
    const target = nextIndex(index, items.length, event.key);
    if (target === null) return;
    event.preventDefault();
    items[target]?.focus();
  };

  const isPin = props.mode === 'pin';
  const title = isPin ? 'Pin a window' : (props.title ?? 'Choose a window');
  const description = isPin
    ? 'Choose a window to keep on top of other windows. Its opacity can be changed on the dashboard.'
    : (props.description ?? 'Choose a running window to use its executable.');

  let content;
  if (loadError !== null) {
    content = (
      <InfoBar
        tone="error"
        title="The window list could not be loaded"
        role="alert"
        actions={
          <Button size="small" onClick={() => void refresh()}>
            Try again
          </Button>
        }
      >
        {loadError}
      </InfoBar>
    );
  } else if (windows === null) {
    content = (
      <div className="picker-placeholder" role="status">
        <span className="spinner" aria-hidden="true" />
        Looking for windows…
      </div>
    );
  } else if (visible.length === 0) {
    content = (
      <div className="picker-placeholder" role="status">
        {windows.length === 0 ? 'No windows that can be managed are open.' : `No windows match “${query.trim()}”.`}
      </div>
    );
  } else {
    content = (
      <ul ref={listRef} className="picker-list" aria-label="Windows">
        {visible.map((window) => {
          const reason = unavailableReason(window);
          const managed = managedByHwnd.get(window.hwnd);
          const noteId = `${baseId}-${window.hwnd}-note`;
          return (
            <li key={window.hwnd}>
              <button
                type="button"
                className={cx('picker-row', reason !== null && 'is-unavailable')}
                data-picker-row="true"
                aria-disabled={reason !== null || undefined}
                aria-describedby={reason !== null ? noteId : undefined}
                aria-busy={pinning === window.hwnd || undefined}
                onClick={() => void choose(window)}
                onKeyDown={onRowKeyDown}
              >
                <WindowIcon exePath={window.exePath} size={24} />
                <span className="picker-row-text">
                  <span className="picker-row-title" title={window.title}>
                    {displayTitle(window.title)}
                  </span>
                  <span className="picker-row-meta">
                    <span className="picker-row-exe">{window.exeName}</span>
                    {managed?.pinned && (
                      <Badge tone="accent" icon="pin">
                        Pinned
                      </Badge>
                    )}
                    {window.managed && !managed?.pinned && <Badge tone="accent">Managed</Badge>}
                    {window.topmost && !window.managed && <Badge>Always on top</Badge>}
                  </span>
                  {reason !== null && (
                    <span id={noteId} className="picker-row-note">
                      {!window.accessible && <Icon name="shield" size={12} />}
                      {reason}
                    </span>
                  )}
                </span>
                {pinning === window.hwnd && <span className="spinner" aria-hidden="true" />}
              </button>
            </li>
          );
        })}
      </ul>
    );
  }

  return (
    <Dialog
      title={title}
      description={description}
      size="large"
      onClose={props.onClose}
      initialFocus={searchRef}
      footer={
        <>
          <span className="dialog-footer-start" role="status">
            {windows !== null && loadError === null && pluralize(visible.length, 'window')}
          </span>
          <Button icon="refresh" busy={loading && windows !== null} onClick={() => void refresh()}>
            Refresh
          </Button>
          <Button onClick={props.onClose}>Cancel</Button>
        </>
      }
    >
      <div className="picker">
        <div className="picker-search">
          <label htmlFor={searchId} className="sr-only">
            Search windows by title or program
          </label>
          <Icon name="search" className="picker-search-icon" />
          <input
            ref={searchRef}
            id={searchId}
            type="search"
            className="text-input"
            placeholder="Search by title or program"
            value={query}
            spellCheck={false}
            autoComplete="off"
            onChange={(event) => setQuery(event.currentTarget.value)}
            onKeyDown={onSearchKeyDown}
          />
        </div>
        {actionError !== null && (
          <InfoBar tone="error" role="alert" compact onDismiss={() => setActionError(null)}>
            {actionError}
          </InfoBar>
        )}
        {content}
      </div>
    </Dialog>
  );
}
