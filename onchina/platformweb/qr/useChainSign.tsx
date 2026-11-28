// 协调管理员冷签、统一链提交及同一签名会话的恢复。

import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import type { AdminAuth } from '../authentication/types';
import { parseSignedReceiptPayload } from './parseSignedPayload';
import { CitizenSignatureModal } from './CitizenSignatureModal';
import { notice } from '../controls/notice';
import { adminHeaders, request } from "../workspace/http";

export type ChainSignResult = {
  accountId: string;
  signature: string;
  /** 原始QR_V1响应，机构接收证明必须原样交回服务端。 */
  response?: string;
  /** 换绑扫码可在同一响应中附带当前账户授权，二者不得拆成两次交互。 */
  currentAccountId?: string;
  currentAccountSignature?: string;
};

/** 所有 OnChina 链交易 prepare 接口共用的最小返回结构。 */
export type ChainSignPrepare = { requestId: string; signRequest: string };

/**
 * 所有 OnChina 链交易共用的提交结果。
 * `citizen` 只在公民占号用途返回，其他业务不得为相同签名响应另建提交实现。
 */
export type ChainSubmitResult<TCitizen = unknown> = {
  purpose: string;
  cidNumber: string;
  txHash: string;
  blockNumber?: number | null;
  citizen?: TCitizen | null;
};

/**
 * 统一链交易提交入口：OnChina 回扫 CitizenWallet 的一次签名响应后，
 * 由后端统一验签、dry-run、提交并等待进块。
 */
export async function submitChainSign<TCitizen = unknown>(
  auth: AdminAuth,
  requestId: string,
  accountId: string,
  signature: string,
): Promise<ChainSubmitResult<TCitizen>> {
  return request<ChainSubmitResult<TCitizen>>('/api/admin/chain/submit', {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      ...adminHeaders(auth),
    },
    body: JSON.stringify({
      requestId: requestId,
      accountId,
      signature,
    }),
  });
}

export type ChainSignOptions = { signal?: AbortSignal; expectedAccountId?: string; title?: string; qrTitle?: string; qrHint?: string; expiresAt?: number; captureResponse?: boolean };

type PendingChainSign = {
  options?: ChainSignOptions;
  cleanup: () => void;
  requestId: string;
  signRequest: string;
  resolve: (value: ChainSignResult) => void;
  reject: (reason?: unknown) => void;
};

export interface UseChainSignResult {
  /** 展示 sign_request 二维码，扫描 CitizenWallet 响应并解析 account_id / signature。 */
  signChain: (requestId: string, signRequest: string, options?: ChainSignOptions) => Promise<ChainSignResult>;
  /** 需在组件 JSX 中渲染的扫码签名弹窗。 */
  chainSignModal: ReactNode;
}

export function useChainSign(title = '管理员公民钱包签名'): UseChainSignResult {
  const [pending, setPending] = useState<PendingChainSign | null>(null);
  const [scanning, setScanning] = useState(false);
  const current = useRef<PendingChainSign | null>(null);
  useEffect(() => () => {
    const active = current.current; current.current = null;
    active?.cleanup(); active?.reject(new Error('签名页面已关闭'));
  }, []);

  const signChain = useCallback(
    (requestId: string, signRequest: string, options?: ChainSignOptions) =>
      new Promise<ChainSignResult>((resolve, reject) => {
        if (options?.signal?.aborted) { reject(new Error('签名操作已取消')); return; }
        if (options?.expiresAt && options.expiresAt * 1000 <= Date.now()) { reject(new Error('签名请求已过期，请重新核对')); return; }
        let timer: ReturnType<typeof setTimeout> | undefined;
        const previous = current.current;
        previous?.cleanup(); previous?.reject(new Error('签名请求已切换'));
        const entry: PendingChainSign = { requestId, signRequest, options, resolve, reject,
          cleanup: () => { options?.signal?.removeEventListener('abort', abort); if (timer) clearTimeout(timer); } };
        const abort = () => {
          if (current.current !== entry) return;
          current.current = null; entry.cleanup(); reject(new Error('签名操作已取消'));
          setPending(null); setScanning(false);
        };
        current.current = entry;
        options?.signal?.addEventListener('abort', abort, { once: true });
        if (options?.expiresAt) timer = setTimeout(() => {
          if (current.current !== entry) return;
          current.current = null; entry.cleanup(); reject(new Error('签名请求已过期，请重新核对'));
          setPending(null); setScanning(false);
        }, Math.max(0, options.expiresAt * 1000 - Date.now()));
        setPending(entry);
      }),
    [],
  );

  const onDetected = useCallback(
    (raw: string) => {
      if (!pending || current.current !== pending) return;
      setScanning(true);
      try {
        const signed = parseSignedReceiptPayload(raw, pending.requestId);
        if (signed.challengeId !== pending.requestId) {
          throw new Error('签名响应与当前请求不匹配');
        }
        if (!signed.accountId) {
          throw new Error('签名响应缺少 account_id');
        }
        if (pending.options?.expectedAccountId && signed.accountId.toLowerCase() !== pending.options.expectedAccountId.toLowerCase()) throw new Error('签名钱包与本次指定账户不一致');
        pending.cleanup(); current.current = null;
        // 统一把新账户签名和可选的当前账户签名交回同一个提交调用。
        pending.resolve({
          accountId: signed.accountId,
          signature: signed.signature,
          ...(pending.options?.captureResponse ? { response: raw } : {}),
          currentAccountId: signed.currentAccountId,
          currentAccountSignature: signed.currentAccountSignature,
        });
        setPending(null);
      } catch (err) {
        pending.cleanup(); current.current = null;
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
    current.current = null; pending?.cleanup();
    pending?.reject(new Error('已取消签名'));
    setPending(null);
    setScanning(false);
  }, [pending]);

  const chainSignModal = (
    <CitizenSignatureModal
      title={pending?.options?.title ?? title}
      open={!!pending}
      onCancel={onCancel}
      qrTitle={pending?.options?.qrTitle ?? "链交易签名二维码"}
      qrValue={pending?.signRequest ?? undefined}
      qrHint={pending?.options?.qrHint ?? "使用管理员公民钱包扫码签名"}
      scannerHint="扫描公民钱包生成的签名响应二维码"
      scannerDisabled={scanning}
      scannerLoading={scanning}
      onDetected={onDetected}
      onScannerError={(msg) => notice.error(msg)}
    />
  );

  return { signChain, chainSignModal };
}
