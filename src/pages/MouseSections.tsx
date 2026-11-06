import { Fragment, useId, useRef, useState } from 'react';
import { Button, IconButton } from '../components/Button';
import { Card, NumberSetting, SwitchSetting } from '../components/Card';
import { GridPreview } from '../components/GridPreview';
import { Icon } from '../components/Icon';
import { TextInput } from '../components/TextInput';
import { WindowPicker } from '../components/WindowPicker';
import { cx } from '../lib/format';
import { buildCheatSheet, MODIFIER_LABELS } from '../lib/gestures';
import { GRID_SIZE_MAX, GRID_SIZE_MIN } from '../lib/pixelGrid';
import { addExcludedApp, patchSection, removeExcludedApp } from '../lib/settingsUpdate';
import type { AlignmentSettings, GestureSettings, SnapSettings } from '../lib/types';
import { useAppState } from '../state/appStateContext';

/** Live summary of the current mouse bindings. */
export function CheatSheetCard({
  gestures,
  snapping,
  alignment,
  managementEnabled,
}: {
  gestures: GestureSettings;
  snapping: SnapSettings;
  alignment: AlignmentSettings;
  managementEnabled: boolean;
}) {
  const sheet = buildCheatSheet(gestures, snapping, alignment, managementEnabled);
  return (
    <Card title="Cheat sheet" icon="info" description="Follows the settings on this page." padded>
      <div className={cx('cheat-sheet', !sheet.active && 'is-inactive')}>
        {sheet.entries.length > 0 && (
          <dl className="cheat-list">
            {sheet.entries.map((entry) => (
              <div key={`${entry.keys.join('+')}-${entry.action}`} className="cheat-row">
                <dt className="cheat-keys">
                  {entry.keys.map((key, index) => (
                    <Fragment key={key}>
                      {index > 0 && (
                        <span className="cheat-plus" aria-hidden="true">
                          +
                        </span>
                      )}
                      <kbd>{key}</kbd>
                    </Fragment>
                  ))}
                </dt>
                <dd className="cheat-action">{entry.action}</dd>
              </div>
            ))}
          </dl>
        )}
        {sheet.notes.length > 0 && (
          <ul className="cheat-notes">
            {sheet.notes.map((note) => (
              <li key={note}>
                <Icon name="info" size={14} />
                {note}
              </li>
            ))}
          </ul>
        )}
      </div>
    </Card>
  );
}

const ORIENTATION_DESCRIPTION =
  'While you move a window, a label shows its position in pixels from the top-left corner of its screen and how ' +
  'far it has moved. When you are not snapping, thin lines mark edges and centres that line up exactly with a ' +
  'screen edge, another window, a centre line or a line of the snapping grid. This never changes where the window goes.';

const GRID_DESCRIPTION =
  'While you move or resize a window, faint lines in the outline and guide colour cover every screen, starting at ' +
  'its top-left corner. Every fifth line is solid. Clicks pass through the grid, and it disappears when you let go.';

/** Pixel orientation and the visible pixel grid shown during gestures. */
export function PixelGridCard() {
  const { settings, commitSettings, editSettings } = useAppState();
  const { gestures, snapping, alignment } = settings;
  const gesturesOff = !gestures.enabled;
  const snapOff = gesturesOff || !snapping.enabled;
  const noMoveButton = gestures.moveButton === null;
  const noButtons = noMoveButton && gestures.resizeButton === null;
  const gridOff = gesturesOff || noButtons || !alignment.showGrid;
  const snapKey = MODIFIER_LABELS[snapping.modifier];
  const orientationHint = gestures.enabled && noMoveButton ? ' Choose a button under “Move with” to use it.' : '';
  const snapHint = gestures.enabled && !snapping.enabled ? ' Turn on Snapping to use this.' : '';

  const setAlignment = (patch: Partial<AlignmentSettings>): void => {
    void commitSettings((current) => patchSection(current, 'alignment', patch));
  };
  const editAlignment = (patch: Partial<AlignmentSettings>): void => {
    editSettings((current) => patchSection(current, 'alignment', patch));
  };

  return (
    <Card
      title="Pixel grid and orientation"
      icon="grid"
      description="Exact positions and a visible grid help you place windows to the pixel."
    >
      <SwitchSetting
        label="Pixel orientation"
        description={`${ORIENTATION_DESCRIPTION}${orientationHint}`}
        disabled={gesturesOff || noMoveButton}
        checked={alignment.pixelOrientation}
        onChange={(pixelOrientation) => setAlignment({ pixelOrientation })}
      />
      <SwitchSetting
        label="Visible pixel grid"
        description={GRID_DESCRIPTION}
        disabled={gesturesOff || noButtons}
        checked={alignment.showGrid}
        onChange={(showGrid) => setAlignment({ showGrid })}
      />
      <NumberSetting
        label="Grid size"
        description="Distance between two lines. Scales with the display scaling of each screen."
        indent
        disabled={gridOff}
        value={alignment.gridSize}
        min={GRID_SIZE_MIN}
        max={GRID_SIZE_MAX}
        unit="px"
        onChange={(gridSize) => editAlignment({ gridSize })}
      />
      <div className="setting-block setting-block--indent">
        <GridPreview gridSize={alignment.gridSize} color={gestures.overlayColor} disabled={gridOff} />
      </div>
      <SwitchSetting
        label="Snap to the pixel grid"
        description={`While the grid is visible, holding ${snapKey} also snaps window edges to its lines when no screen edge, other window or other snap target is within reach.${snapHint}`}
        indent
        disabled={snapOff || !alignment.showGrid}
        checked={alignment.snapToGrid}
        onChange={(snapToGrid) => setAlignment({ snapToGrid })}
      />
    </Card>
  );
}

/** Applications in which mouse gestures never start. */
export function ExcludedAppsCard() {
  const { settings, commitSettings } = useAppState();
  const apps = settings.gestures.excludedApps;
  const [text, setText] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [adding, setAdding] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const inputId = useId();
  const errorId = useId();

  const add = async (raw: string): Promise<boolean> => {
    const result = addExcludedApp(apps, raw);
    if (!result.ok) {
      setError(result.error);
      return false;
    }
    setError(null);
    setAdding(true);
    const ok = await commitSettings((current) => {
      const next = addExcludedApp(current.gestures.excludedApps, raw);
      return next.ok ? patchSection(current, 'gestures', { excludedApps: next.list }) : current;
    });
    setAdding(false);
    return ok;
  };

  const remove = (name: string, index: number): void => {
    // Keep keyboard focus in the list (next item, previous item, or the input).
    const buttons = Array.from(listRef.current?.querySelectorAll<HTMLButtonElement>('button') ?? []);
    const neighbour = buttons[index + 1] ?? buttons[index - 1];
    if (neighbour) neighbour.focus();
    else inputRef.current?.focus();
    void commitSettings((current) => {
      const list = current.gestures.excludedApps;
      const position = list.indexOf(name);
      if (position === -1) return current;
      return patchSection(current, 'gestures', { excludedApps: removeExcludedApp(list, position) });
    });
  };

  return (
    <Card
      title="Excluded applications"
      icon="ban"
      description="Mouse gestures never start in these programs, for example apps that use the same mouse and key combination."
    >
      <form
        className="exclusion-form"
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          void add(text).then((ok) => {
            if (ok) setText('');
          });
        }}
      >
        <label htmlFor={inputId} className="sr-only">
          Program to exclude
        </label>
        <TextInput
          ref={inputRef}
          id={inputId}
          value={text}
          placeholder="Program, for example blender.exe"
          maxLength={1024}
          invalid={error !== null}
          aria-describedby={error ? errorId : undefined}
          onChange={(event) => {
            setText(event.currentTarget.value);
            if (error) setError(null);
          }}
        />
        <Button type="submit" icon="plus" busy={adding}>
          Add
        </Button>
        <Button icon="window" onClick={() => setPickerOpen(true)}>
          Choose from running windows…
        </Button>
      </form>
      {error && (
        <p id={errorId} className="field-error exclusion-error" role="alert">
          <Icon name="error" size={14} />
          {error}
        </p>
      )}
      {apps.length === 0 ? (
        <p className="exclusion-empty">No excluded applications. Gestures work in every eligible window.</p>
      ) : (
        <ul ref={listRef} className="exclusion-list" aria-label="Excluded applications">
          {apps.map((name, index) => (
            <li key={name} className="exclusion-item">
              <Icon name="ban" size={14} className="exclusion-icon" />
              <code className="exclusion-name" title={name}>
                {name}
              </code>
              <IconButton icon="trash" size="small" label={`Remove ${name}`} onClick={() => remove(name, index)} />
            </li>
          ))}
        </ul>
      )}
      {pickerOpen && (
        <WindowPicker
          mode="select"
          title="Exclude a program"
          description="Mouse gestures will be ignored in every window of the program you choose."
          onClose={() => setPickerOpen(false)}
          onSelect={(window) => {
            void add(window.exeName);
          }}
        />
      )}
    </Card>
  );
}
