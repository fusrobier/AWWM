import { useId } from 'react';
import { Button } from '../components/Button';
import {
  Card,
  ColorSetting,
  NumberSetting,
  SelectSetting,
  SettingRow,
  SliderSetting,
  SwitchSetting,
} from '../components/Card';
import { RadioGroup } from '../components/Checkbox';
import { InfoBar, PageHeader, SaveIndicator } from '../components/Feedback';
import type { SelectOption } from '../components/Select';
import { useResource } from '../hooks/useResource';
import { loadKeyNames } from '../lib/backend';
import { formatPixels } from '../lib/format';
import {
  BUTTON_LABELS,
  MODIFIER_LABELS,
  MODIFIERS,
  MOUSE_BUTTONS,
  snapModifierOptions,
} from '../lib/gestures';
import { HOTKEY_ACTIONS } from '../lib/hotkeys';
import {
  patchSection,
  setActivationModifier,
  setGestureButton,
  setHotkey,
  setSnapModifier,
  type GestureButtonSlot,
} from '../lib/settingsUpdate';
import type { GestureScope, GestureSettings, MouseButton, SnapSettings } from '../lib/types';
import { useAppState } from '../state/appStateContext';
import { HotkeyEditor } from './HotkeyEditor';
import { CheatSheetCard, ExcludedAppsCard, PixelGridCard } from './MouseSections';

type ButtonChoice = MouseButton | 'none';

const SCOPE_OPTIONS: ReadonlyArray<{ value: GestureScope; label: string; description: string }> = [
  { value: 'allWindows', label: 'All eligible windows', description: 'Any normal window under the pointer.' },
  { value: 'pinnedOnly', label: 'Pinned windows only', description: 'Only windows pinned by AWWM.' },
];

function buttonOptions(other: MouseButton | null, otherUse: string): Array<SelectOption<ButtonChoice>> {
  return [
    ...MOUSE_BUTTONS.map((button) => ({
      value: button,
      label: button === other ? `${BUTTON_LABELS[button]} (used to ${otherUse})` : BUTTON_LABELS[button],
      disabled: button === other,
    })),
    { value: 'none', label: 'Disabled' },
  ];
}

export function MousePage() {
  const { snapshot, settings, commitSettings, editSettings, saveStatus, saveError } = useAppState();
  const keyNames = useResource(loadKeyNames);
  const scopeLabelId = useId();
  const gestures = settings.gestures;
  const snapping = settings.snapping;
  const gesturesOff = !gestures.enabled;
  const snapOff = gesturesOff || !snapping.enabled;

  const setGestures = (patch: Partial<GestureSettings>): void => {
    void commitSettings((current) => patchSection(current, 'gestures', patch));
  };
  const setSnapping = (patch: Partial<SnapSettings>): void => {
    void commitSettings((current) => patchSection(current, 'snapping', patch));
  };
  const editSnapping = (patch: Partial<SnapSettings>): void => {
    editSettings((current) => patchSection(current, 'snapping', patch));
  };
  const setButton = (slot: GestureButtonSlot, choice: ButtonChoice): void => {
    void commitSettings((current) => setGestureButton(current, slot, choice === 'none' ? null : choice));
  };

  const snapKey = MODIFIER_LABELS[snapping.modifier];

  return (
    <>
      <PageHeader
        title="Mouse controls"
        description="Move and resize any window by dragging inside it while holding a key, with optional snapping."
        status={<SaveIndicator status={saveStatus} error={saveError} />}
      />

      <CheatSheetCard
        gestures={gestures}
        snapping={snapping}
        alignment={settings.alignment}
        managementEnabled={snapshot.managementEnabled}
      />

      <Card title="Move and resize" icon="mouse">
        <SwitchSetting
          label="Mouse gestures"
          description="Drag anywhere inside a window while holding the activation key."
          checked={gestures.enabled}
          onChange={(enabled) => setGestures({ enabled })}
        />
        <SelectSetting
          label="Activation key"
          description="Hold this key, then press a mouse button inside a window."
          disabled={gesturesOff}
          value={gestures.modifier}
          options={MODIFIERS.map((modifier) => ({ value: modifier, label: MODIFIER_LABELS[modifier] }))}
          onChange={(modifier) => {
            void commitSettings((current) => setActivationModifier(current, modifier));
          }}
        />
        <SelectSetting<ButtonChoice>
          label="Move with"
          description="Mouse button that moves the window."
          disabled={gesturesOff}
          value={gestures.moveButton ?? 'none'}
          options={buttonOptions(gestures.resizeButton, 'resize')}
          onChange={(choice) => setButton('moveButton', choice)}
        />
        <SelectSetting<ButtonChoice>
          label="Resize with"
          description="Mouse button that resizes from the nearest edge or corner."
          disabled={gesturesOff}
          value={gestures.resizeButton ?? 'none'}
          options={buttonOptions(gestures.moveButton, 'move')}
          onChange={(choice) => setButton('resizeButton', choice)}
        />
        <SettingRow label="Works on" labelId={scopeLabelId} disabled={gesturesOff}>
          <RadioGroup
            name="gesture-scope"
            value={gestures.scope}
            options={SCOPE_OPTIONS}
            disabled={gesturesOff}
            aria-labelledby={scopeLabelId}
            onChange={(scope) => setGestures({ scope })}
          />
        </SettingRow>
        <SwitchSetting
          label="Restore maximized windows when moving"
          description="A maximized window is restored to its normal size and follows the pointer."
          disabled={gesturesOff}
          checked={gestures.restoreMaximizedOnMove}
          onChange={(restoreMaximizedOnMove) => setGestures({ restoreMaximizedOnMove })}
        />
        <SwitchSetting
          label="Ignore full-screen windows"
          description="Games and videos that cover a whole screen are never moved."
          disabled={gesturesOff}
          checked={gestures.ignoreFullscreen}
          onChange={(ignoreFullscreen) => setGestures({ ignoreFullscreen })}
        />
        <SwitchSetting
          label="Show outline and size while resizing"
          disabled={gesturesOff}
          checked={gestures.showResizeOverlay}
          onChange={(showResizeOverlay) => setGestures({ showResizeOverlay })}
        />
        <ColorSetting
          label="Outline and guide colour"
          disabled={gesturesOff}
          value={gestures.overlayColor}
          onChange={(overlayColor) => editSettings((current) => patchSection(current, 'gestures', { overlayColor }))}
        />
      </Card>

      <Card title="Snapping" icon="snap" description={`Hold ${snapKey} during a move or resize to align the window.`}>
        <SwitchSetting
          label="Snapping"
          description="Windows snap to nearby edges while the snap key is held."
          disabled={gesturesOff}
          checked={snapping.enabled}
          onChange={(enabled) => setSnapping({ enabled })}
        />
        <SelectSetting
          label="Snap key"
          description={`Cannot be the activation key (${MODIFIER_LABELS[gestures.modifier]}).`}
          disabled={snapOff}
          value={snapping.modifier}
          options={snapModifierOptions(gestures.modifier).map((modifier) => ({
            value: modifier,
            label: MODIFIER_LABELS[modifier],
          }))}
          onChange={(modifier) => {
            void commitSettings((current) => setSnapModifier(current, modifier));
          }}
        />
        <SliderSetting
          label="Snap distance"
          description="How close an edge must be before the window snaps to it."
          disabled={snapOff}
          value={snapping.distance}
          min={2}
          max={100}
          formatValue={formatPixels}
          onCommit={(distance) => setSnapping({ distance })}
        />
        <SwitchSetting
          label="Screen edges"
          disabled={snapOff}
          checked={snapping.monitorEdges}
          onChange={(monitorEdges) => setSnapping({ monitorEdges })}
        />
        <SwitchSetting
          label="Work area edges"
          description="Edges of the screen area not covered by the taskbar."
          disabled={snapOff}
          checked={snapping.workAreaEdges}
          onChange={(workAreaEdges) => setSnapping({ workAreaEdges })}
        />
        <SwitchSetting
          label="Other windows"
          description="Edges of visible windows next to the one you drag."
          disabled={snapOff}
          checked={snapping.windowEdges}
          onChange={(windowEdges) => setSnapping({ windowEdges })}
        />
        <SwitchSetting
          label="Centre lines"
          description="The horizontal and vertical centre of each work area."
          disabled={snapOff}
          checked={snapping.centerLines}
          onChange={(centerLines) => setSnapping({ centerLines })}
        />
        <SwitchSetting
          label="Grid"
          description="Snap to an invisible grid that divides each work area."
          disabled={snapOff}
          checked={snapping.gridEnabled}
          onChange={(gridEnabled) => setSnapping({ gridEnabled })}
        />
        <NumberSetting
          label="Grid columns"
          indent
          disabled={snapOff || !snapping.gridEnabled}
          value={snapping.gridColumns}
          min={2}
          max={24}
          onChange={(gridColumns) => editSnapping({ gridColumns })}
        />
        <NumberSetting
          label="Grid rows"
          indent
          disabled={snapOff || !snapping.gridEnabled}
          value={snapping.gridRows}
          min={2}
          max={24}
          onChange={(gridRows) => editSnapping({ gridRows })}
        />
        <SwitchSetting
          label="Show snap guides"
          description="Draws a thin line where the window snapped."
          disabled={snapOff}
          checked={snapping.showGuides}
          onChange={(showGuides) => setSnapping({ showGuides })}
        />
      </Card>

      <PixelGridCard />

      <ExcludedAppsCard />

      <Card
        title="Keyboard shortcuts"
        icon="keyboard"
        description="Global shortcuts work in every application. Each needs at least one modifier key."
      >
        {keyNames.resource.status === 'error' && (
          <div className="card-inset">
            <InfoBar
              tone="error"
              title="The list of keys could not be loaded"
              actions={
                <Button size="small" onClick={keyNames.reload}>
                  Try again
                </Button>
              }
            >
              {keyNames.resource.error.message}
            </InfoBar>
          </div>
        )}
        {HOTKEY_ACTIONS.map((action) => (
          <HotkeyEditor
            key={action}
            action={action}
            saved={settings.hotkeys[action]}
            status={snapshot.hotkeys.find((item) => item.action === action)}
            hotkeys={settings.hotkeys}
            keyNames={keyNames.resource}
            managementEnabled={snapshot.managementEnabled}
            onSave={(hotkey) => commitSettings((current) => setHotkey(current, action, hotkey))}
          />
        ))}
        <NumberSetting
          label="Opacity step"
          description="How much the opacity shortcuts change the active window per press."
          value={settings.hotkeys.opacityStep}
          min={1}
          max={50}
          unit="%"
          onChange={(opacityStep) => editSettings((current) => patchSection(current, 'hotkeys', { opacityStep }))}
        />
      </Card>
    </>
  );
}
