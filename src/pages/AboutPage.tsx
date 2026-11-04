import { Button } from '../components/Button';
import { Card, SettingRow } from '../components/Card';
import { InfoBar, PageHeader } from '../components/Feedback';
import { AppLogo, Icon, type IconName } from '../components/Icon';
import { useCommandRunner } from '../hooks/useCommand';
import { useResource, type Resource } from '../hooks/useResource';
import { api } from '../lib/api';
import { loadAppInfo } from '../lib/backend';
import type { AppInfo, DocumentKind } from '../lib/types';

const DOCUMENTS: ReadonlyArray<{ kind: DocumentKind; label: string; description: string; icon: IconName }> = [
  { kind: 'userGuide', label: 'User guide', description: 'Pinning, rules, gestures and shortcuts', icon: 'document' },
  { kind: 'limitations', label: 'Windows limitations', description: 'Elevated apps, games, special windows', icon: 'shield' },
  { kind: 'architecture', label: 'Architecture', description: 'How AWWM works inside', icon: 'general' },
  { kind: 'readme', label: 'Readme', description: 'Overview and building from source', icon: 'info' },
  { kind: 'license', label: 'License', description: 'MIT License text', icon: 'document' },
];

export function AboutPage() {
  const { resource: info, reload } = useResource(loadAppInfo);
  const { run, isBusy } = useCommandRunner();

  const openDocument = (kind: DocumentKind, label: string): void => {
    void run(`doc-${kind}`, () => api.openDocumentation(kind), `${label} could not be opened`);
  };

  return (
    <>
      <PageHeader title="About" />

      <Card>
        <div className="about-hero">
          <AppLogo size={56} />
          <div className="about-hero-text">
            <h2 className="about-name">AWWM — Advanced Window Manager</h2>
            <p className="about-version" aria-live="polite">
              {info.status === 'ready' && <>Version {info.data.version}</>}
              {info.status === 'loading' && 'Loading version…'}
              {info.status === 'error' && 'Version unavailable'}
            </p>
            <p className="about-description">
              Keeps chosen windows on top, makes them translucent, applies automatic rules as windows open, and
              moves or resizes any window with the mouse, with snapping.
            </p>
            <p className="about-meta">
              <span>License: {info.status === 'ready' && info.data.license ? info.data.license : 'MIT'}</span>
              <span aria-hidden="true">·</span>
              <span>Built with Rust, Tauri and React</span>
            </p>
          </div>
        </div>
      </Card>

      {info.status === 'error' && (
        <InfoBar
          tone="error"
          title="Application details could not be loaded"
          actions={
            <Button size="small" onClick={reload}>
              Try again
            </Button>
          }
        >
          {info.error.message}
        </InfoBar>
      )}

      <Card title="Documentation" icon="document" description="Opens in your default viewer.">
        <ul className="doc-grid">
          {DOCUMENTS.map((doc) => (
            <li key={doc.kind}>
              <button
                type="button"
                className="doc-button"
                aria-busy={isBusy(`doc-${doc.kind}`) || undefined}
                onClick={() => openDocument(doc.kind, doc.label)}
              >
                {isBusy(`doc-${doc.kind}`) ? (
                  <span className="spinner" aria-hidden="true" />
                ) : (
                  <Icon name={doc.icon} size={20} className="doc-icon" />
                )}
                <span className="doc-text">
                  <span className="doc-label">{doc.label}</span>
                  <span className="doc-description">{doc.description}</span>
                </span>
                <Icon name="external" size={14} className="doc-external" />
              </button>
            </li>
          ))}
        </ul>
      </Card>

      <Card title="Files and folders" icon="folder">
        <SettingRow
          label="Settings folder"
          description={<PathText info={info} pick={(data) => data.configDir} />}
        >
          <Button
            icon="folder"
            busy={isBusy('config')}
            onClick={() => void run('config', () => api.openLocation('config'), 'The settings folder could not be opened')}
          >
            Open
          </Button>
        </SettingRow>
        <SettingRow label="Settings file" description={<PathText info={info} pick={(data) => data.settingsPath} />} />
        <SettingRow label="Log folder" description={<PathText info={info} pick={(data) => data.logDir} />}>
          <Button
            icon="folder"
            busy={isBusy('logs')}
            onClick={() => void run('logs', () => api.openLocation('logs'), 'The log folder could not be opened')}
          >
            Open
          </Button>
        </SettingRow>
      </Card>

      <p className="about-disclaimer">Independent project; not affiliated with or endorsed by Microsoft.</p>
    </>
  );
}

function PathText({ info, pick }: { info: Resource<AppInfo>; pick: (data: AppInfo) => string }) {
  if (info.status === 'loading') return <span>Loading…</span>;
  if (info.status === 'error') return <span>Unavailable</span>;
  return (
    <code className="path-text selectable" title={pick(info.data)}>
      {pick(info.data)}
    </code>
  );
}
