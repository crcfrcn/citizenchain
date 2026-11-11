import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

type Props = {
  title: string;
  items: readonly ReactNode[];
};

export function MetricHelp({ title, items }: Props) {
  const [open, setOpen] = useState(false);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const titleId = useId();

  useEffect(() => {
    if (!open) return;
    const dialog = dialogRef.current;
    dialog?.showModal();
    return () => dialog?.close();
  }, [open]);

  return (
    <>
      <button
        type="button"
        className="metric-help-button"
        aria-label={title}
        aria-haspopup="dialog"
        onClick={() => setOpen(true)}
      >
        说明
      </button>
      {open && createPortal(
        <dialog
          ref={dialogRef}
          className="metric-help-dialog"
          aria-labelledby={titleId}
          onClick={(event) => {
            if (event.target === event.currentTarget) setOpen(false);
          }}
          onCancel={(event) => {
            event.preventDefault();
            setOpen(false);
          }}
        >
          <div className="metric-help-content">
            <h2 id={titleId}>{title}</h2>
            <ol>
              {items.map((item, index) => <li key={index}>{item}</li>)}
            </ol>
          </div>
        </dialog>,
        document.body,
      )}
    </>
  );
}
