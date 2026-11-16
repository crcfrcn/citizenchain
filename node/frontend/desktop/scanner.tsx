import { ScannerView, createQrAssembler } from "@gmb/scanner-react";
import { parseAddressQr, type AddressScanResult } from "../client/parseAddressQr";
import { useState, useCallback, useMemo, useEffect, useRef } from "react";

import { invoke } from '../client/tauri';

// 节点产品扫码外壳：视觉遮罩留在产品内，设备识别统一交给仓库共享适配器。


type ScannerProps = {
  onScan: (data: string) => void;
  onError: (error: string) => void;
};

export function QrScanner({ onScan, onError }: ScannerProps) {
  const [ready, setReady] = useState(false);
  const [cameraError, setCameraError] = useState<string | null>(null);

  const session = useRef(0);
  useEffect(() => () => { session.current += 1; }, []);
  const assemble = useMemo(() => createQrAssembler(frames => invoke<string | null>('join_qr_frames', {frames})), [cameraError]);
  const detected = async (raw: string): Promise<void | false> => {
    const current = session.current;
    try {
      const complete = await assemble(raw);
      if (current !== session.current) return;
      if (complete === null) return false;
      onScan(complete);
    } catch (error) {
      if (current === session.current) onError(String(error));
    }
  };
  return (
    <div className="qr-scanner-wrapper">
      {!cameraError && (
        <ScannerView
          className="qr-scanner-video"
          onRawValue={detected}
          onReady={() => setReady(true)}
          onFailure={(failure) => {
            session.current += 1;
            if (failure.kind === 'camera-unavailable' || failure.kind === 'operation-failed') {
              setReady(false);
              setCameraError(failure.message);
            } else {
              onError(failure.message);
            }
          }}
        />
      )}
      <div className="qr-scanner-overlay">
        {ready ? <div className="qr-scanner-frame" /> : (
          <div className="qr-scanner-notice">
            <p role="status">{cameraError ?? '正在准备摄像头'}</p>
            {cameraError && (
              <button type="button" className="cancel-button" onClick={() => { session.current += 1; setCameraError(null); }}>
                重新打开相机
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

// 扫码填入地址弹层：复用 QrScanner，解析结果后回调。


type AddressProps = {
  onResult: (result: AddressScanResult) => void;
  onClose: () => void;
};

export function AddressScanModal({ onResult, onClose }: AddressProps) {
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
