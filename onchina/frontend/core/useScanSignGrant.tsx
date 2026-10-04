// 通用「扫码签名」授权 hook。PASSKEY_COLD_SIGN 安全动作(如创建机构/创建账户)
// 通过它拿到 signWithScan 回调:prepare 后弹出公民钱包二维码,扫描签名响应,
// 解析出 account_id/signature 回传给 securityApi 的 createScanSignSecurityGrant 去 commit。
//
// 用法:
//   const { signWithScan, scanSignModal } = useScanSignGrant();
//   await createAccount(auth, cidNumber, accountName, signWithScan);
//   ...在 JSX 末尾渲染 {scanSignModal}

import { useCallback, useState, type ReactNode } from 'react';
import type { PrepareAdminActionOutput } from '../admins/securityApi';
import { parseSignedReceiptPayload } from '../utils/parseSignedPayload';
import { CitizenSignatureModal } from './CitizenSignatureModal';
import { notice } from '../utils/notice';

type PendingScanSign = {
  prepared: PrepareAdminActionOutput;
  resolve: (value: { account_id: string; signature: string }) => void;
  reject: (reason?: unknown) => void;
};

export interface UseScanSignGrantResult {
  /** 传给 createScanSignSecurityGrant / create* API 的扫码签名解析回调。 */
  signWithScan: (prepared: PrepareAdminActionOutput) => Promise<{ account_id: string; signature: string }>;
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
      new Promise<{ account_id: string; signature: string }>((resolve, reject) => {
        setPending({ prepared, resolve, reject });
      }),
    [],
  );

  const onDetected = useCallback(
    async (raw: string) => {
      if (!pending) return;
      setScanning(true);
      try {
        const signed = parseSignedReceiptPayload(raw, pending.prepared.action_id);
        if (signed.challenge_id !== pending.prepared.action_id) {
          throw new Error('签名响应与当前请求不匹配');
        }
        if (!signed.account_id) {
          throw new Error('签名响应缺少 account_id');
        }
        pending.resolve({ account_id: signed.account_id, signature: signed.signature });
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
      qrValue={pending?.prepared.sign_request ?? undefined}
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
