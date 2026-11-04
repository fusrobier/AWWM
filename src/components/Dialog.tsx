import {
  useEffectEvent,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
} from 'react';
import { cx } from '../lib/format';
import { Button, IconButton } from './Button';

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusableIn(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    // Only elements that belong to this dialog (not to a nested one) and are rendered.
    (element) => element.closest('dialog') === container && element.getClientRects().length > 0,
  );
}

interface DialogProps {
  title: string;
  description?: ReactNode;
  /** Requested close (Esc, close button). The parent unmounts the dialog. */
  onClose: () => void;
  children?: ReactNode;
  footer?: ReactNode;
  size?: 'small' | 'medium' | 'large';
  /** Element focused when the dialog opens; defaults to the first focusable element. */
  initialFocus?: RefObject<HTMLElement | null>;
  /** Prevents closing while an operation is running. */
  closeDisabled?: boolean;
  /** Render as a form; `onSubmit` runs on Enter / submit buttons. */
  onSubmit?: () => void;
  role?: 'dialog' | 'alertdialog';
}

/**
 * Modal dialog built on the native <dialog> element (top layer, inert background).
 * Mount it to open, unmount it to close. Traps Tab, closes on Esc and returns focus to
 * the element that had it before opening.
 */
export function Dialog({
  title,
  description,
  onClose,
  children,
  footer,
  size = 'medium',
  initialFocus,
  closeDisabled = false,
  onSubmit,
  role = 'dialog',
}: DialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const descriptionId = useId();

  const requestClose = useEffectEvent(() => {
    if (!closeDisabled) onClose();
  });

  useLayoutEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return undefined;
    const previouslyFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    if (!dialog.open) dialog.showModal();
    const target = initialFocus?.current ?? focusableIn(dialog).find((el) => !el.dataset.dialogClose);
    target?.focus();

    // Esc fires `cancel`; keep React in control of the open state.
    const onCancel = (event: Event): void => {
      event.preventDefault();
      requestClose();
    };
    dialog.addEventListener('cancel', onCancel);

    return () => {
      dialog.removeEventListener('cancel', onCancel);
      if (dialog.open) dialog.close();
      if (previouslyFocused?.isConnected) {
        previouslyFocused.focus();
      } else {
        // The opener disappeared (e.g. its row was deleted): keep focus in the page.
        document.getElementById('main-content')?.focus();
      }
    };
    // Runs once per mount: the dialog opens when mounted and closes when unmounted.
  }, []);

  const handleKeyDown = (event: KeyboardEvent<HTMLDialogElement>): void => {
    if (event.key !== 'Tab') return;
    const dialog = dialogRef.current;
    // Ignore keys that bubble up from a nested dialog.
    if (!dialog || !(event.target instanceof Element) || event.target.closest('dialog') !== dialog) return;
    const items = focusableIn(dialog);
    const first = items[0];
    const last = items[items.length - 1];
    if (!first || !last) {
      event.preventDefault();
      return;
    }
    if (event.shiftKey && (document.activeElement === first || !dialog.contains(document.activeElement))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  const body = (
    <>
      <header className="dialog-header">
        <div className="dialog-header-text">
          <h2 id={titleId} className="dialog-title">
            {title}
          </h2>
          {description && (
            <div id={descriptionId} className="dialog-description">
              {description}
            </div>
          )}
        </div>
        <IconButton
          label="Close"
          icon="close"
          data-dialog-close="true"
          disabled={closeDisabled}
          onClick={onClose}
        />
      </header>
      {children !== undefined && <div className="dialog-body">{children}</div>}
      {footer && <footer className="dialog-footer">{footer}</footer>}
    </>
  );

  return (
    <dialog
      ref={dialogRef}
      className={cx('dialog', size !== 'medium' && `dialog--${size}`)}
      role={role === 'alertdialog' ? 'alertdialog' : undefined}
      aria-labelledby={titleId}
      aria-describedby={description ? descriptionId : undefined}
      onKeyDown={handleKeyDown}
      onClose={() => {
        // The browser may force-close a dialog (repeated Esc); keep React in sync.
        if (dialogRef.current && !dialogRef.current.open) onClose();
      }}
    >
      {onSubmit ? (
        <form
          className="dialog-form"
          noValidate
          onSubmit={(event) => {
            event.preventDefault();
            onSubmit();
          }}
        >
          {body}
        </form>
      ) : (
        body
      )}
    </dialog>
  );
}

interface ConfirmDialogProps {
  title: string;
  message: ReactNode;
  confirmLabel: string;
  /** Uses a red confirm button and focuses Cancel initially. */
  destructive?: boolean;
  /**
   * Runs the confirmed action. Resolve `true` to close the dialog; `false` keeps it open
   * (the caller has already reported the error).
   */
  onConfirm: () => Promise<boolean>;
  /** Close request (Cancel, Esc, or after a successful confirm). */
  onClose: () => void;
}

export function ConfirmDialog({
  title,
  message,
  confirmLabel,
  destructive = false,
  onConfirm,
  onClose,
}: ConfirmDialogProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const [busy, setBusy] = useState(false);

  const confirm = async (): Promise<void> => {
    setBusy(true);
    const done = await onConfirm();
    if (done) onClose();
    else setBusy(false);
  };

  return (
    <Dialog
      title={title}
      size="small"
      role="alertdialog"
      description={message}
      onClose={onClose}
      closeDisabled={busy}
      initialFocus={destructive ? cancelRef : undefined}
      footer={
        <>
          <Button
            variant={destructive ? 'danger' : 'accent'}
            busy={busy}
            onClick={() => {
              void confirm();
            }}
          >
            {confirmLabel}
          </Button>
          <Button ref={cancelRef} onClick={onClose} disabled={busy}>
            Cancel
          </Button>
        </>
      }
    />
  );
}
