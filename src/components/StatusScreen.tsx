import { AppLogo } from './Icon';
import { Button } from './Button';

interface StatusScreenProps {
  kind: 'loading' | 'error';
  title: string;
  message?: string;
  detail?: string;
  onRetry?: () => void;
}

/** Full-window loading and failure states shown before the snapshot is available. */
export function StatusScreen({ kind, title, message, detail, onRetry }: StatusScreenProps) {
  if (kind === 'loading') {
    return (
      <div className="status-screen">
        <div className="status-panel" role="status">
          <span className="spinner spinner--large" aria-hidden="true" />
          <p className="status-panel-text">{title}</p>
        </div>
      </div>
    );
  }

  return (
    <main className="status-screen">
      <div className="status-panel" role="alert">
        <AppLogo size={48} />
        <h1 className="status-panel-title">{title}</h1>
        {message && <p className="status-panel-text">{message}</p>}
        {detail && (
          <p className="status-panel-detail selectable">{detail}</p>
        )}
        {onRetry && (
          <Button variant="accent" icon="refresh" onClick={onRetry}>
            Try again
          </Button>
        )}
      </div>
    </main>
  );
}
