// 管理员列表页：两列网格展示所有管理员，每个管理员一个卡片。
// 从机构详情页点击"管理员列表"入口卡片进入。
import { useEffect, useState, useCallback } from 'react';
import { sanitizeError } from '../tauri';
import { CitizenSignatureModal } from '../shared/qr/CitizenSignatureModal';
import { adminsChangeApi as api } from './api';
import { InstitutionAssignmentCard } from './InstitutionAssignmentCard';
import type {
  ActivateRequestResult,
  ActivatedAdmin,
  InstitutionAdminsRef,
  InstitutionDetail,
} from './types';
import './styles.css';

type Props = {
  cidNumber: string;
  accountRef: InstitutionAdminsRef;
  onBack: () => void;
};

type ActivateStep = 'idle' | 'qr' | 'verifying' | 'done' | 'error';

export function AdminListPage({ cidNumber, accountRef, onBack }: Props) {
  const [detail, setDetail] = useState<InstitutionDetail | null>(null);
  const [activatedAdmins, setActivatedAdmins] = useState<ActivatedAdmin[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  // 激活流程状态
  const [activateStep, setActivateStep] = useState<ActivateStep>('idle');
  const [activatePubkey, setActivatePubkey] = useState('');
  const [activateRequest, setActivateRequest] = useState<ActivateRequestResult | null>(null);
  const [activateCountdown, setActivateCountdown] = useState(90);
  const [activateError, setActivateError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    Promise.all([
      api.getInstitutionDetail(cidNumber),
      api.getActivatedAdmins(cidNumber, accountRef).catch(() => [] as ActivatedAdmin[]),
    ])
      .then(([d, aa]) => {
        setDetail(d);
        setActivatedAdmins(aa);
        setError(null);
      })
      .catch((e) => setError(sanitizeError(e)))
      .finally(() => setLoading(false));
  }, [cidNumber, accountRef.cidNumber, accountRef.institutionCode]);

  // 激活倒计时
  useEffect(() => {
    if (activateStep !== 'qr') return;
    if (activateCountdown <= 0) {
      setActivateError('签名请求已过期，请重新操作');
      setActivateStep('error');
      return;
    }
    const timer = setTimeout(() => setActivateCountdown(c => c - 1), 1000);
    return () => clearTimeout(timer);
  }, [activateStep, activateCountdown]);

  const startActivation = useCallback(async (account_id: string) => {
    setActivatePubkey(account_id);
    setActivateError(null);
    try {
      const result = await api.buildActivateAdminRequest(account_id, cidNumber, accountRef);
      setActivateRequest(result);
      setActivateCountdown(90);
      setActivateStep('qr');
    } catch (e) {
      setActivateError(sanitizeError(e));
      setActivateStep('error');
    }
  }, [cidNumber, accountRef.cidNumber, accountRef.institutionCode]);

  const handleActivateScan = useCallback(async (responseJson: string) => {
    if (!activateRequest) return;
    setActivateStep('verifying');
    try {
      const result = await api.verifyActivateAdmin(
        activateRequest.requestId,
        activatePubkey,
        activateRequest.expectedPayloadHash,
        activateRequest.payloadHex,
        responseJson,
      );
      setActivatedAdmins(prev => [...prev.filter(a => a.account_id !== result.account_id), result]);
      setActivateStep('done');
      setTimeout(() => setActivateStep('idle'), 2000);
    } catch (e) {
      setActivateError(sanitizeError(e));
      setActivateStep('error');
    }
  }, [activateRequest, activatePubkey]);

  const closeActivation = useCallback(() => {
    if (activateStep === 'verifying') return;
    setActivateStep('idle');
  }, [activateStep]);

  if (loading) {
    return <div className="governance-section"><p>加载中…</p></div>;
  }

  if (error || !detail) {
    return (
      <div className="governance-section">
        <button className="back-button" onClick={onBack}>← 返回</button>
        {error && <div className="error">{error}</div>}
      </div>
    );
  }

  const activatedCount = activatedAdmins.length;

  return (
    <div className="governance-section">
      <button className="back-button" onClick={onBack}>← 返回机构详情</button>

      <div className="admin-list-header">
        <h2>管理员列表</h2>
        <span className="admin-list-summary">
          共 {detail.admins.length} 人{activatedCount > 0 && `，已激活 ${activatedCount}`}
        </span>
      </div>

      {detail.admins.length === 0 ? (
        <p className="no-data">暂无数据（需节点运行后查询链上数据）</p>
      ) : (
        <div className="admin-grid">
          {detail.admins.map((admin, i) => {
            const accountId = admin.account_id;
            const isActivated = activatedAdmins.some(
              a => a.account_id === accountId
            );
            return (
              <InstitutionAssignmentCard
                key={accountId}
                admin={admin}
                index={i + 1}
                balanceFen={admin.balanceFen}
                className={isActivated ? 'admin-card-activated' : ''}
                action={
                  isActivated ? (
                    <span className="activated-tag">已激活</span>
                  ) : (
                    <button
                      className="activate-button"
                      onClick={() => startActivation(accountId)}
                    >激活</button>
                  )
                }
              />
            );
          })}
        </div>
      )}

      <CitizenSignatureModal
        open={activateStep !== 'idle' && activateRequest != null}
        title="激活管理员"
        qrValue={activateRequest?.requestJson ?? ''}
        countdownSeconds={activateCountdown}
        status={
          activateStep === 'verifying'
            ? 'submitting'
            : activateStep === 'done'
              ? 'success'
              : activateStep === 'error'
                ? 'error'
                : 'ready'
        }
        statusTitle={
          activateStep === 'verifying'
            ? '正在验证管理员签名'
            : activateStep === 'done'
              ? '管理员激活成功'
              : undefined
        }
        error={activateError}
        onScan={handleActivateScan}
        onScanError={(e) => { setActivateError(e); setActivateStep('error'); }}
        onCancel={closeActivation}
      />

    </div>
  );
}
