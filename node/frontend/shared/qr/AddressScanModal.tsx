// 扫码填入地址弹层：复用 QrScanner，解析结果后回调。
import { useState, useCallback } from 'react';
import { QrScanner } from './QrScanner';
import { parseAddressQr, type AddressScanResult } from './parseAddressQr';

type Props = {
  onResult: (result: AddressScanResult) => void;
  onClose: () => void;
};

export function AddressScanModal({ onResult, onClose }: Props) {
  const [error, setError] = useState<string | null>(null);
  const [scanKey, setScanKey] = useState(0);

  const handleScan = useCallback((data: string) => {
    try {
      const result = parseAddressQr(data);
      onResult(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : '解析失败');
      // 重新挂载 QrScanner 继续扫
      setScanKey((k) => k + 1);
    }
  }, [onResult]);

  const handleError = useCallback((msg: string) => {
    setError(msg);
  }, []);

  return (
    <div className="address-scan-modal-mask" onClick={onClose}>
      <div className="address-scan-modal" onClick={(e) => e.stopPropagation()}>
        <p className="qr-instruction">扫描收款地址二维码</p>
        <QrScanner key={scanKey} onScan={handleScan} onError={handleError} />
        {error && <p className="address-scan-error">{error}</p>}
        <button className="cancel-button" onClick={onClose}>取消</button>
      </div>
    </div>
  );
}
