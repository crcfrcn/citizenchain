// 公民钱包签名弹窗外壳。内容统一交给 CitizenSignaturePanel,
// 避免登录、扫码签名、管理员动作各自维护不同的扫码界面。

import { Modal } from 'antd';
import type { CitizenSignaturePanelProps } from './CitizenSignaturePanel';
import { CitizenSignaturePanel } from './CitizenSignaturePanel';
import { CID_MODAL_Z_INDEX } from './modalStack';

export interface CitizenSignatureModalProps extends CitizenSignaturePanelProps {
  open: boolean;
  title: string;
  onCancel: () => void;
  zIndex?: number;
}

export function CitizenSignatureModal({
  open,
  title,
  onCancel,
  zIndex = CID_MODAL_Z_INDEX.securitySignature,
  ...panelProps
}: CitizenSignatureModalProps) {
  return (
    <Modal
      title={<span style={{ display: 'block', textAlign: 'center', fontWeight: 600 }}>{title}</span>}
      open={open}
      onCancel={onCancel}
      footer={null}
      destroyOnClose
      maskClosable={false}
      keyboard={false}
      width={760}
      zIndex={zIndex}
    >
      <CitizenSignaturePanel {...panelProps} />
    </Modal>
  );
}
