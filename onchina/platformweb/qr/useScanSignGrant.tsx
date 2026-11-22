// 为敏感动作显示扫码签名并关联账户和原始响应。

import { useCallback, useState, type ReactNode } from 'react';
import type { PrepareAdminActionOutput } from '../authentication/security';
import { parseSignedReceiptPayload } from './parseSignedPayload';
import { CitizenSignatureModal } from './CitizenSignatureModal';
import { notice } from '../controls/notice';

type PendingScanSign = {
  prepared: PrepareAdminActionOutput;
  resolve: (value: { accountId: string; signature: string }) => void;
  reject: (reason?: unknown) => void;
};

export interface UseScanSignGrantResult {
  /** 传给 createScanSignSecurityGrant / create* API 的扫码签名解析回调。 */
  signWithScan: (prepared: PrepareAdminActionOutput) => Promise<{ accountId: string; signature: string }>;
  /** 需在组件 JSX 中渲染的扫码签名弹窗。 */
  scanSignModal: ReactNode;
}

export function useScanSignGrant(
  title = '公民钱包签名确认',
): UseScanSignGrantResult {
  const [pending, setPending] = useState<PendingScanSign | null>(null);
  const [scanning, setScanning] = useState(false);

  const signWithScan = useCallback(
    (prepared: PrepareAdminActionOutput) =>
      new Promise<{ accountId: string; signature: string }>((resolve, reject) => {
        setPending({ prepared, resolve, reject });
      }),
    [],
  );

  const onDetected = useCallback(
    async (raw: string) => {
      if (!pending) return;
      setScanning(true);
      try {
        const signed = parseSignedReceiptPayload(raw, pending.prepared.actionId);
        if (signed.challengeId !== pending.prepared.actionId) {
          throw new Error('签名响应与当前请求不匹配');
        }
        if (!signed.accountId) {
          throw new Error('签名响应缺少 account_id');
        }
        pending.resolve({ accountId: signed.accountId, signature: signed.signature });
        setPending(null);
      } catch (err) {
        pending.reject(err);
        setPending(null);
        notice.error(err, '');
      } finally {
        setScanning(false);
      }
    },
    [pending],
  );

  const onCancel = useCallback(() => {
    pending?.reject(new Error('已取消签名确认'));
    setPending(null);
    setScanning(false);
  }, [pending]);

  const scanSignModal = (
    <CitizenSignatureModal
      title={title}
      open={!!pending}
      onCancel={onCancel}
      qrTitle="签名二维码"
      qrValue={pending?.prepared.signRequest ?? undefined}
      qrHint="使用管理员公民钱包扫码签名"
      scannerHint="扫描公民钱包生成的签名响应二维码"
      scannerDisabled={scanning}
      scannerLoading={scanning}
      onDetected={onDetected}
      onScannerError={(msg) => notice.error(msg)}
    />
  );

  return { signWithScan, scanSignModal };
}
