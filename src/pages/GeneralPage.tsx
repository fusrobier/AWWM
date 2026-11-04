import { useState } from 'react';
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
import { ConfirmDialog } from '../components/Dialog';
import { InfoBar, PageHeader, SaveIndicator } from '../components/Feedback';
import { Icon } from '../components/Icon';
import { useCommandRunner } from '../hooks/useCommand';
import { api } from '../lib/api';
import { isNearlyInvisible } from '../lib/opacity';
import { patchSection } from '../lib/settingsUpdate';
import { THEME_LABELS } from '../lib/theme';
import type { GeneralSettings, LogLevel, PinningSettings, Theme } from '../lib/types';
import { useAppState } from '../state/appStateContext';

const THEMES: readonly Theme[] = ['system', 'light', 'dark'];

const LOG_LEVEL_LABELS: Record<LogLevel, string> = {
  error: 'Errors only',
  warn: 'Warnings and errors',
  info: 'Information (default)',
  debug: 'Debug',
  trace: 'Trace (very detailed)',
};

const LOG_LEVELS: readonly LogLevel[] = ['error', 'warn', 'info', 'debug', 'trace'];

export function GeneralPage() {
  const {
    snapshot,
    settings,
    commitSettings,
    editSettings,
    saveStatus,
    saveError,
    resetSettings,
    exportSettings,
    importSettings,
    dismissedWarning,
    dismissWarning,
  } = useAppState();
  const [backupBusy, setBackupBusy] = useState<'export' | 'import' | null>(null);
  const runBackup = async (kind: 'export' | 'import'): Promise<void> => {
    if (backupBusy) return;
    setBackupBusy(kind);
    try {
      await (kind === 'export' ? exportSettings() : importSettings());
    } finally {
      setBackupBusy(null);
    }
  };
  const { run, isBusy } = useCommandRunner();
  const [confirmReset, setConfirmReset] = useState(false);
  const general = settings.general;
  const pinning = settings.pinning;

  const setGeneral = (patch: Partial<GeneralSettings>): void => {
    void commitSettings((current) => patchSection(current, 'general', patch));
  };
  const setPinning = (patch: Partial<PinningSettings>): void => {
    void commitSettings((current) => patchSection(current, 'pinning', patch));
  };
  const editPinning = (patch: Partial<PinningSettings>): void => {
    editSettings((current) => patchSection(current, 'pinning', patch));
  };

  const warning = snapshot.settingsWarning;
  const showWarning = warning !== null && warning !== dismissedWarning;

  return (
    <>
      <PageHeader
        title="General"
        description="Startup, recovery, pinned-window appearance and maintenance."
        status={<SaveIndicator status={saveStatus} error={saveError} />}
      />

      {showWarning && (
        <InfoBar tone="warning" title="Settings were not loaded" onDismiss={dismissWarning}>
          {warning}
        </InfoBar>
      )}

      <Card title="Startup" icon="startup">
        <SwitchSetting
          label="Start with Windows"
          description="Start AWWM automatically when you sign in."
          checked={general.startWithWindows}
          onChange={(startWithWindows) => setGeneral({ startWithWindows })}
        />
        <SwitchSetting
          label="Start minimized to the tray"
          description="Do not open this window when AWWM starts."
          checked={general.startMinimized}
          onChange={(startMinimized) => setGeneral({ startMinimized })}
        />
        <SwitchSetting
          label="Apply rules at startup"
          description="Apply automatic rules to windows that are already open when AWWM starts."
          checked={general.applyRulesOnStartup}
          onChange={(applyRulesOnStartup) => setGeneral({ applyRulesOnStartup })}
        />
      </Card>

      <Card title="Background and recovery" icon="restore">
        <SwitchSetting
          label="Keep running in the background when this window is closed"
          description="Pins, rules, gestures and shortcuts keep working from the tray."
          checked={general.runInBackground}
          onChange={(runInBackground) => setGeneral({ runInBackground })}
        />
        <SwitchSetting
          label="Restore windows when exiting"
          description="Release all managed windows (always-on-top and opacity) when AWWM exits."
          checked={general.restoreOnExit}
          onChange={(restoreOnExit) => setGeneral({ restoreOnExit })}
        />
        <SwitchSetting
          label="Recover managed windows after a restart or crash"
          description="Windows that were managed when AWWM stopped are adopted again, so they can always be restored."
          checked={general.recoverSession}
          onChange={(recoverSession) => setGeneral({ recoverSession })}
        />
        <NumberSetting
          label="Reattach grace period"
          description="When a managed window closes, a new window of the same program that appears within this time inherits its pin and opacity. 0 turns this off."
          value={general.reattachGraceSecs}
          min={0}
          max={600}
          unit="seconds"
          onChange={(reattachGraceSecs) =>
            editSettings((current) => patchSection(current, 'general', { reattachGraceSecs }))
          }
        />
      </Card>

      <Card title="Pinned windows" icon="pin">
        <SwitchSetting
          label="Show a frame around pinned windows"
          description="A coloured, click-through border marks windows that stay on top."
          checked={pinning.showBorder}
          onChange={(showBorder) => setPinning({ showBorder })}
        />
        <ColorSetting
          label="Frame colour"
          indent
          disabled={!pinning.showBorder}
          value={pinning.borderColor}
          onChange={(borderColor) => editPinning({ borderColor })}
        />
        <NumberSetting
          label="Frame thickness"
          indent
          disabled={!pinning.showBorder}
          value={pinning.borderThickness}
          min={1}
          max={16}
          unit="px"
          onChange={(borderThickness) => editPinning({ borderThickness })}
        />
        <SliderSetting
          label="Default opacity"
          description="Applied when you pin a window manually."
          value={pinning.defaultOpacity}
          min={0}
          max={100}
          onCommit={(defaultOpacity) => setPinning({ defaultOpacity })}
          footer={(live) =>
            isNearlyInvisible(live) ? (
              <p className="setting-warning">
                <Icon name="warning" size={14} />
                Newly pinned windows will be nearly invisible. Use the dashboard or the tray to restore them.
              </p>
            ) : null
          }
        />
        <SwitchSetting
          label="Reassert always-on-top and opacity"
          description="Re-apply them when an application resets its own window state."
          checked={pinning.reassertState}
          onChange={(reassertState) => setPinning({ reassertState })}
        />
      </Card>

      <Card title="Appearance and diagnostics" icon="palette">
        <SelectSetting
          label="Theme"
          value={general.theme}
          options={THEMES.map((theme) => ({ value: theme, label: THEME_LABELS[theme] }))}
          onChange={(theme) => setGeneral({ theme })}
        />
        <SelectSetting
          label="Log level"
          description="More detailed logs help when reporting a problem."
          value={general.logLevel}
          options={LOG_LEVELS.map((level) => ({ value: level, label: LOG_LEVEL_LABELS[level] }))}
          onChange={(logLevel) => setGeneral({ logLevel })}
        />
      </Card>

      <Card title="Backup" icon="document">
        <SettingRow
          label="Export settings"
          description="Saves all settings, keyboard shortcuts and automatic rules to a file of your choice."
        >
          <Button icon="document" busy={backupBusy === 'export'} onClick={() => void runBackup('export')}>
            Export…
          </Button>
        </SettingRow>
        <SettingRow
          label="Import settings"
          description="Replaces the current settings and rules with those from an exported file. Window management stays on or off as it is now."
        >
          <Button icon="folder" busy={backupBusy === 'import'} onClick={() => void runBackup('import')}>
            Import…
          </Button>
        </SettingRow>
      </Card>

      <Card title="Maintenance" icon="folder">
        <SettingRow label="Settings folder" description="Contains settings.json and its backup.">
          <Button
            icon="folder"
            busy={isBusy('config')}
            onClick={() => void run('config', () => api.openLocation('config'), 'The settings folder could not be opened')}
          >
            Open settings folder
          </Button>
        </SettingRow>
        <SettingRow label="Log folder" description="Daily log files from the last seven days.">
          <Button
            icon="document"
            busy={isBusy('logs')}
            onClick={() => void run('logs', () => api.openLocation('logs'), 'The log folder could not be opened')}
          >
            Open log folder
          </Button>
        </SettingRow>
        <SettingRow
          label="Reset all settings"
          description="Restores every setting, shortcut and automatic rule to its default."
        >
          <Button variant="danger-text" icon="restore" onClick={() => setConfirmReset(true)}>
            Reset all settings…
          </Button>
        </SettingRow>
      </Card>

      {confirmReset && (
        <ConfirmDialog
          title="Reset all settings?"
          message="Every setting, keyboard shortcut and automatic rule is replaced with its default. This cannot be undone."
          confirmLabel="Reset"
          destructive
          onConfirm={resetSettings}
          onClose={() => setConfirmReset(false)}
        />
      )}
    </>
  );
}
