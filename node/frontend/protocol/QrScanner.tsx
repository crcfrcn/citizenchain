// 节点产品扫码外壳：视觉遮罩留在产品内，设备识别统一交给仓库共享适配器。
import { ScannerView } from '@gmb/scanner-react';

type Props = {
  onScan: (data: string) => void;
  onError: (error: string) => void;
};

export function QrScanner({ onScan, onError }: Props) {
  return (
    <div className="qr-scanner-wrapper">
      <ScannerView
        className="qr-scanner-video"
        onRawValue={onScan}
        onFailure={(failure) => onError(failure.message)}
      />
      <div className="qr-scanner-overlay">
        <div className="qr-scanner-frame" />
      </div>
    </div>
  );
}
