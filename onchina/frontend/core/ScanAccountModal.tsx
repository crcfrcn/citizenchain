// 通用“扫码识别账户”弹窗。这里要的是「一个账户」，因此只接受 QR_V1 `k=5` 账户码，
// 其 body.account_id 本身就是规范 account_id，直接回填给业务表单。
// 用户码(k=3)表达「人」、收款码(k=4)表达「一笔收款请求」，都不是账户声明，一律拒绝。
// 设备层统一复用仓库级 jsQR + canvas 适配器；本弹窗只判断账户码业务语义。

import { useEffect, useState } from 'react';
import { ScannerView } from '@gmb/scanner-react';
import { Button, Modal, Typography } from 'antd';
import { parseQrEnvelope, QrParseError, type AccountIdCodeBody } from './citizenQr';
import { CID_MODAL_Z_INDEX } from './modalStack';

export function ScanAccountModal(props: {
  open: boolean;
  onClose: () => void;
  onResolved: (account_id: string) => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const [scanAttempt, setScanAttempt] = useState(0);

  useEffect(() => {
    if (!props.open) {
      setError(null);
    }
  }, [props.open]);

  const handleRawValue = (raw: string) => {
    try {
      const env = parseQrEnvelope(raw);
      if (env.kind !== 'account_id_code') {
        setError('请扫描账户码（钱包 → 账户详情右上角二维码）');
        setScanAttempt((attempt) => attempt + 1);
        return;
      }
      const account_id = (env.body as AccountIdCodeBody).account_id;
      props.onResolved(account_id);
    } catch (e) {
      if (e instanceof QrParseError) {
        setError(e.message);
      } else {
        setError('二维码不是有效 QR_V1 格式');
      }
      // 共享适配器领取一次原文后会停止；错误码必须开启新扫描会话。
      setScanAttempt((attempt) => attempt + 1);
    }
  };

  return (
    <Modal
      title={<div style={{ textAlign: 'center', width: '100%' }}>扫描账户码</div>}
      open={props.open}
      onCancel={props.onClose}
      footer={[
        <Button key="cancel" onClick={props.onClose}>
          取消
        </Button>,
      ]}
      destroyOnClose
      width={420}
      zIndex={CID_MODAL_Z_INDEX.accountScan}
    >
      <div
        style={{
          width: '100%',
          aspectRatio: '1 / 1',
          background: 'linear-gradient(145deg, #0f172a, #1e293b)',
          borderRadius: 12,
          overflow: 'hidden',
          position: 'relative',
        }}
      >
        <ScannerView
          key={scanAttempt}
          active={props.open}
          style={{ width: '100%', height: '100%', objectFit: 'cover' }}
          onRawValue={handleRawValue}
          onFailure={(failure) => setError(failure.message)}
        />
      </div>
      {error && (
        <Typography.Paragraph type="danger" style={{ marginTop: 12, marginBottom: 0 }}>
          {error}
        </Typography.Paragraph>
      )}
    </Modal>
  );
}
