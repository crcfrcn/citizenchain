// 私权机构详情页只调度私权布局,不得承载公权机构业务。

import React, { useCallback, useEffect, useState } from 'react';
import { Typography } from 'antd';
import { getInstitution, type InstitutionDetail } from './common/api';
import type { AdminAuth } from '../auth/types';
import { notice } from '../utils/notice';
import { PrivateDetailLayout } from './PrivateDetailLayout';
import {
  commitAdminAction,
  prepareAdminAction,
  type AdminActionType,
  type AdminSecurityGrantOutput,
} from '../admins/securityApi';
import { parseSignedReceiptPayload } from '../utils/parseSignedPayload';
import { CitizenSignatureModal } from '../core/CitizenSignatureModal';
import {
  institutionDetailCacheKey,
  readCachedInstitutionDetail,
  writeCachedInstitutionDetail,
} from '../china/metaCache';

interface Props {
  auth: AdminAuth;
  cidNumber: string;
  canWrite: boolean;
  onBack: () => void;
}

type SecurityModalState = {
  actionId: string;
  signRequest: string;
  payloadHash: string;
  resolve: (value: AdminSecurityGrantOutput) => void;
  reject: (reason?: unknown) => void;
};

export const PrivateDetailPage: React.FC<Props> = ({ auth, cidNumber, canWrite, onBack }) => {
  const detailCacheKey = institutionDetailCacheKey(auth, cidNumber);
  const [detail, setDetail] = useState<InstitutionDetail | null>(() =>
    readCachedInstitutionDetail(detailCacheKey),
  );
  const [loading, setLoading] = useState(false);
  const [securityCommitLoading, setSecurityCommitLoading] = useState(false);
  const [securityModal, setSecurityModal] = useState<SecurityModalState | null>(null);

  const load = useCallback(() => {
    const cached = readCachedInstitutionDetail(detailCacheKey);
    if (cached) {
      setDetail(cached);
      setLoading(false);
    } else {
      setDetail(null);
      setLoading(true);
    }
    getInstitution(auth, cidNumber)
      .then((next) => {
        setDetail(next);
        writeCachedInstitutionDetail(detailCacheKey, next);
      })
      .catch(() => {
        // 详情后台刷新失败时保留缓存,避免切页时闪断。
      })
      .finally(() => {
        if (!cached) setLoading(false);
      });
  }, [auth.access_token, detailCacheKey, cidNumber]);

  useEffect(() => {
    load();
  }, [load]);

  const runScanSignGrant = async (
    actionType: AdminActionType,
    payload: unknown,
  ): Promise<AdminSecurityGrantOutput> => {
    const prepared = await prepareAdminAction(auth, actionType, payload);
    if (prepared.auth_type !== 'PASSKEY_COLD_SIGN' || !prepared.sign_request) {
      throw new Error('该操作缺少公民钱包签名请求');
    }
    return new Promise<AdminSecurityGrantOutput>((resolve, reject) => {
      setSecurityModal({
        actionId: prepared.action_id,
        signRequest: prepared.sign_request || '',
        payloadHash: prepared.payload_hash,
        resolve,
        reject,
      });
    });
  };

  const handleSecuritySignedResponse = useCallback(async (raw: string) => {
    if (!securityModal) return;
    setSecurityCommitLoading(true);
    try {
      const signed = parseSignedReceiptPayload(raw, securityModal.actionId);
      if (signed.challenge_id !== securityModal.actionId) {
        throw new Error('签名响应与当前请求不匹配');
      }
      if (!signed.account_id) {
        throw new Error('签名响应缺少 account_id');
      }
      const grant = await commitAdminAction<AdminSecurityGrantOutput>(auth, {
        action_id: securityModal.actionId,
        account_id: signed.account_id,
        signature: signed.signature,
        payload_hash: securityModal.payloadHash,
      });
      securityModal.resolve(grant);
      setSecurityModal(null);
    } catch (err) {
      securityModal.reject(err);
      notice.error(err, '');
    } finally {
      setSecurityCommitLoading(false);
    }
  }, [auth, securityModal]);

  const inst = detail?.institution;

  return (
    <div>
      {loading && !inst && <Typography.Text type="secondary">加载中...</Typography.Text>}

      {inst && detail && (
        <PrivateDetailLayout
          auth={auth}
          detail={detail}
          canWrite={canWrite}
          loading={loading}
          onReload={load}
          createScanSignGrant={runScanSignGrant}
          onBack={onBack}
        />
      )}

      <CitizenSignatureModal
        title="公民钱包签名确认"
        open={!!securityModal}
        onCancel={() => {
          securityModal?.reject(new Error('已取消签名确认'));
          setSecurityModal(null);
          setSecurityCommitLoading(false);
        }}
        qrTitle="签名二维码"
        qrValue={securityModal?.signRequest}
          qrHint="使用当前注册局管理员公民钱包扫码签名"
        scannerHint="扫描公民钱包生成的签名响应二维码"
        scannerDisabled={securityCommitLoading}
        scannerLoading={securityCommitLoading}
        onDetected={handleSecuritySignedResponse}
        onScannerError={(msg) => notice.error(msg)}
      />
    </div>
  );
};
