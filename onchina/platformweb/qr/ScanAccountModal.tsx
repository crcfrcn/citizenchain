// 扫描固定账户码并向业务表单交付规范账户标识。

import { useEffect, useState, useRef } from 'react';
import { ScannerView, createQrAssembler } from '@gmb/scanner-react';
import { Button, Modal, Typography } from 'antd';
import { parseQrEnvelope, QrParseError, type AccountIdCodeBody } from './citizenQr';
import { CID_MODAL_Z_INDEX } from '../controls/modalStack';

export function ScanAccountModal(props: {
  open: boolean;
  onClose: () => void;
  onResolved: (accountId: string) => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const [scanAttempt, setScanAttempt] = useState(0);
  const [progress,setProgress]=useState(0);
  const assembler=useRef<ReturnType<typeof createQrAssembler>|null>(null);
  useEffect(()=>{
    setProgress(0);
    const current=assembler.current ??= createQrAssembler(setProgress);
    return()=>{current.close();if(assembler.current===current)assembler.current=null;};
  },[props.open,scanAttempt]);


  useEffect(() => {
    if (!props.open) {
      setError(null);
    }
  }, [props.open]);

  const handleRawValue = async (raw: string):Promise<void|false> => {
    try {
      const complete=await (assembler.current ??= createQrAssembler(setProgress))(raw);
      if(complete===null)return false;
      const env = parseQrEnvelope(complete);
      if (env.kind !== 'account_id_code') {
        setError('请扫描账户码（钱包 → 账户详情右上角二维码）');
        setScanAttempt((attempt) => attempt + 1);
        return;
      }
      const accountId = (env.body as AccountIdCodeBody).account_id;
      props.onResolved(accountId);
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
      {progress>0 && progress<100 && <Typography.Text role="status" type="secondary" style={{fontSize:13,display:'block',textAlign:'center',marginTop:14}}>正在接收账户码 {progress}%</Typography.Text>}
      {error && (
        <Typography.Paragraph type="danger" style={{ marginTop: 12, marginBottom: 0 }}>
          {error}
        </Typography.Paragraph>
      )}
    </Modal>
  );
}
