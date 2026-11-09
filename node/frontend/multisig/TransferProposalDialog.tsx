import { useEffect, useId, useRef, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

export type TransferProposalStep = 'form' | 'qr' | 'submit' | 'done' | 'error';

type Props = {
  title: string;
  institutionName: string;
  step: TransferProposalStep;
  generating: boolean;
  signDisabled: boolean;
  onCancel: () => void;
  onSign: () => void;
  onComplete: () => void;
  onRetry?: () => void;
  completeLabel?: string;
  scanner?: ReactNode;
  children: ReactNode;
};

export function TransferProposalDialog({
  title, institutionName, step, generating, signDisabled,
  onCancel, onSign, onComplete, onRetry, completeLabel = '完成', scanner, children,
}: Props) {
  const titleId = useId();
  const dialogRef = useRef<HTMLDialogElement>(null);
  const titleRef = useRef<HTMLHeadingElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const wasScannerOpen = useRef(false);
  const scannerOpen = Boolean(scanner);

  useEffect(() => {
    const dialog = dialogRef.current;
    dialog?.showModal();
    return () => dialog?.close();
  }, []);

  useEffect(() => {
    bodyRef.current?.scrollTo?.(0, 0);
    titleRef.current?.focus({ preventScroll: true });
  }, [step]);

  useEffect(() => {
    if (scannerOpen) {
      dialogRef.current?.querySelector<HTMLButtonElement>('.address-scan-modal button')?.focus();
    } else if (wasScannerOpen.current) {
      dialogRef.current?.querySelector<HTMLButtonElement>('.scan-icon-btn')?.focus();
    }
    wasScannerOpen.current = scannerOpen;
  }, [scannerOpen]);

  return createPortal(
    <dialog
      ref={dialogRef}
      className="institution-transfer-dialog"
      data-step={step}
      aria-labelledby={titleId}
      onCancel={(event) => event.preventDefault()}
    >
      <div className="institution-transfer-panel" inert={scannerOpen}>
        <header>
          <h2 id={titleId} ref={titleRef} tabIndex={-1}>{title}</h2>
          <p className="proposal-institution-name">{institutionName}</p>
        </header>
        <div ref={bodyRef} className="institution-transfer-body" aria-busy={generating || step === 'submit'}>
          {children}
        </div>
        {step !== 'submit' && (
          <footer className="institution-transfer-actions">
            {step !== 'done' && (
              <button type="button" className="institution-transfer-cancel" onClick={onCancel} disabled={generating}>取消</button>
            )}
            {step === 'form' && (
              <button type="button" className="institution-transfer-primary" onClick={onSign} disabled={signDisabled || generating}>
                {generating ? '生成中…' : '签名'}
              </button>
            )}
            {step === 'error' && onRetry && (
              <button type="button" className="institution-transfer-primary" onClick={onRetry}>重试</button>
            )}
            {step === 'done' && (
              <button type="button" className="institution-transfer-primary" onClick={onComplete}>{completeLabel}</button>
            )}
          </footer>
        )}
      </div>
      {scanner}
    </dialog>,
    document.body,
  );
}
