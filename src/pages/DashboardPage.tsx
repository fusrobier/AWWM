import { useState, type ReactNode } from 'react';
import { Button } from '../components/Button';
import { Card, SwitchSetting } from '../components/Card';
import { ConfirmDialog } from '../components/Dialog';
import { Badge, EmptyState, PageHeader } from '../components/Feedback';
import { Icon, type IconName } from '../components/Icon';
import { WindowPicker } from '../components/WindowPicker';
import { api } from '../lib/api';
import { pluralize } from '../lib/format';
import { summarizeGestures } from '../lib/gestures';
import { describeHotkeyStatus, formatHotkey, HOTKEY_ACTION_INFO, HOTKEY_ACTIONS } from '../lib/hotkeys';
import { useAppState, useNavigation } from '../state/appStateContext';
import { useToasts } from '../state/toastContext';
import { ManagedWindowRow } from './ManagedWindowRow';

export function DashboardPage() {
  const { snapshot, settings, setManagementEnabled, updateManaged } = useAppState();
  const { navigate } = useNavigation();
  const { notify, notifyError } = useToasts();
  const [pickerOpen, setPickerOpen] = useState(false);
  const [confirmUnpinAll, setConfirmUnpinAll] = useState(false);
  const [toggling, setToggling] = useState(false);

  const managed = snapshot.managed;
  const enabled = snapshot.managementEnabled;
  const togglePin = settings.hotkeys.togglePin;

  const toggleManagement = async (next: boolean): Promise<void> => {
    setToggling(true);
    await setManagementEnabled(next);
    setToggling(false);
  };

  const unpinAll = async (): Promise<boolean> => {
    const count = managed.length;
    try {
      await api.unpinAll();
    } catch (error) {
      notifyError(error, 'The windows were not released');
      return false;
    }
    updateManaged(() => []);
    notify('success', `Released ${pluralize(count, 'window')}: always-on-top and opacity are restored.`);
    return true;
  };

  const openPicker = (): void => setPickerOpen(true);

  return (
    <>
      <PageHeader
        title="Dashboard"
        description="Windows AWWM manages right now, and the shortcuts that control them."
      />

      <Card>
        <SwitchSetting
          icon="power"
          label="Window management"
          description={
            enabled
              ? 'On: AWWM applies pins, opacity, automatic rules, mouse gestures and keyboard shortcuts.'
              : 'Paused: automatic rules, mouse gestures, keyboard shortcuts and pin frames are inactive.'
          }
          checked={enabled}
          busy={toggling}
          onChange={(next) => void toggleManagement(next)}
        />
      </Card>

      <Card title="Shortcuts and gestures" description={enabled ? undefined : 'Inactive while window management is paused.'}>
        <ul className="summary-list">
          <SummaryItem
            icon="keyboard"
            title="Keyboard"
            action={
              <Button variant="link" onClick={() => navigate('mouse')}>
                Change
              </Button>
            }
          >
            <HotkeySummary />
          </SummaryItem>
          <SummaryItem
            icon="mouse"
            title="Mouse"
            action={
              <Button variant="link" onClick={() => navigate('mouse')}>
                Change
              </Button>
            }
          >
            <p>{summarizeGestures(settings.gestures, settings.snapping)}</p>
          </SummaryItem>
          <SummaryItem
            icon="rules"
            title="Automatic rules"
            action={
              <Button variant="link" onClick={() => navigate('rules')}>
                Manage
              </Button>
            }
          >
            <p>{describeRules(settings.rules.filter((rule) => rule.enabled).length, settings.rules.length)}</p>
          </SummaryItem>
        </ul>
      </Card>

      <Card
        title={
          <>
            Managed windows <span className="count-pill">{managed.length}</span>
          </>
        }
        description="Pinned windows stay on top of other windows. Unpin releases a window completely."
        actions={
          <>
            {managed.length > 0 && (
              <Button icon="unpin" onClick={() => setConfirmUnpinAll(true)}>
                Unpin all
              </Button>
            )}
            <Button variant="accent" icon="pin" onClick={openPicker}>
              Pin a window…
            </Button>
          </>
        }
      >
        {managed.length === 0 ? (
          <EmptyState
            icon="pin"
            title="No managed windows"
            actions={
              <>
                <Button variant="accent" icon="pin" onClick={openPicker}>
                  Pin a window…
                </Button>
                <Button icon="rules" onClick={() => navigate('rules')}>
                  Create a rule
                </Button>
              </>
            }
          >
            <p>
              Pin a window to keep it above all others and adjust its opacity here.
              {togglePin ? (
                <>
                  {' '}
                  You can also press <kbd>{formatHotkey(togglePin)}</kbd> in any window
                </>
              ) : (
                ' You can also set a keyboard shortcut under Mouse controls'
              )}
              , or let automatic rules pin windows as they open.
            </p>
          </EmptyState>
        ) : (
          <ul className="managed-list" aria-label="Managed windows">
            {managed.map((window) => (
              <ManagedWindowRow key={window.hwnd} window={window} rules={settings.rules} />
            ))}
          </ul>
        )}
      </Card>

      {pickerOpen && <WindowPicker mode="pin" onClose={() => setPickerOpen(false)} />}
      {confirmUnpinAll && (
        <ConfirmDialog
          title="Unpin all windows?"
          message={`All ${pluralize(managed.length, 'managed window')} will be released: always-on-top and opacity are restored.`}
          confirmLabel="Unpin all"
          destructive
          onConfirm={unpinAll}
          onClose={() => setConfirmUnpinAll(false)}
        />
      )}
    </>
  );
}

function describeRules(enabledCount: number, total: number): string {
  if (total === 0) return 'No automatic rules yet. Rules pin windows or change their opacity as they open.';
  if (enabledCount === 0) return `${pluralize(total, 'rule')}, none enabled.`;
  return `${enabledCount} of ${pluralize(total, 'rule')} enabled.`;
}

function SummaryItem({
  icon,
  title,
  action,
  children,
}: {
  icon: IconName;
  title: string;
  action: ReactNode;
  children: ReactNode;
}) {
  return (
    <li className="summary-item">
      <Icon name={icon} size={20} className="summary-icon" />
      <div className="summary-text">
        <p className="summary-title">{title}</p>
        <div className="summary-body">{children}</div>
      </div>
      <div className="summary-action">{action}</div>
    </li>
  );
}

function HotkeySummary() {
  const { snapshot, settings } = useAppState();
  const lines = HOTKEY_ACTIONS.flatMap((action) => {
    const configured = settings.hotkeys[action];
    if (configured === null) return [];
    const status = snapshot.hotkeys.find((item) => item.action === action);
    return [{ action, label: status?.label ?? formatHotkey(configured), status }];
  });

  if (lines.length === 0) return <p>No keyboard shortcuts are set.</p>;

  return (
    <ul className="hotkey-summary">
      {lines.map(({ action, label, status }) => {
        const described = status ? describeHotkeyStatus(status) : null;
        return (
          <li key={action}>
            <span>
              <kbd>{label}</kbd> {HOTKEY_ACTION_INFO[action].summary}
            </span>
            {described?.tone === 'ok' && (
              <Badge tone="success" icon="check">
                Active
              </Badge>
            )}
            {described?.tone === 'inactive' && <Badge>Inactive</Badge>}
            {described?.tone === 'error' && status?.error && (
              <span className="hotkey-error">
                <Icon name="error" size={14} />
                {status.error}
              </span>
            )}
          </li>
        );
      })}
    </ul>
  );
}
