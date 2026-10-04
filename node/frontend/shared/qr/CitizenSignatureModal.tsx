import type { ReactNode } from 'react';

import {
  CitizenSignaturePanel,
  type CitizenSignatureStatus,
} from './CitizenSignaturePanel';

type Props = {
  open: boolean;
  title: string;
  qrValue: string;
  countdownSeconds?: number;
  status?: CitizenSignatureStatus;
  statusTitle?: string;
  statusMessage?: ReactNode;
  error?: string | null;
  onScan: (responseJson: string) => void;
  onScanError: (error: string) => void;
  onCancel: () => void;
};

/**
 * Node 桌面端统一扫码签名弹窗。
 *
 * 外层弹窗只处理居中遮罩和关闭行为，签名展示交给
 * CitizenSignaturePanel，业务页面只需要传入二维码内容和扫码回调。
 */
export function CitizenSignatureModal({
  open,
  title,
  qrValue,
  countdownSeconds,
  status,
  statusTitle,
  statusMessage,
  error,
  onScan,
  onScanError,
  onCancel,
}: Props) {
  if (!open) return null;

  return (
    <div className="citizen-signature-modal-overlay" onClick={onCancel}>
      <div className="citizen-signature-modal" onClick={(event) => event.stopPropagation()}>
        <div className="citizen-signature-modal-head">
          <h3>{title}</h3>
          <button type="button" className="citizen-signature-modal-close" onClick={onCancel}>
            ×
          </button>
        </div>
        <CitizenSignaturePanel
          qrValue={qrValue}
          countdownSeconds={countdownSeconds}
          status={status}
          statusTitle={statusTitle}
          statusMessage={statusMessage}
          error={error}
          onScan={onScan}
          onScanError={onScanError}
        />
      </div>
    </div>
  );
}
