import { useCallback, useEffect, useRef, useState } from 'react';
import { sanitizeError } from '../../tauri';
import { accountIdToSs58 } from '../../shared/ss58';
import { CitizenSignaturePanel } from '../../shared/qr/CitizenSignaturePanel';
import type { AdminSignerMatch } from '../types';
import { grandpaKeyChangeApi as api } from './api';
import type {
  GrandpaChangeKind,
  GrandpaKeyChangeRequest,
  GrandpaKeyChangeStatus,
} from './types';

type FlowStep = 'form' | 'qr' | 'submit' | 'done' | 'error';

type Props = {
  actorCidNumber: string;
  adminSigners: AdminSignerMatch[];
  onBack: () => void;
  onSuccess: () => void;
};

function kindLabel(kind: GrandpaChangeKind | null): string {
  return kind === 'emergency_recovery' ? '紧急恢复（本机构内部投票）' : '正常更换（单个委员）';
}

export function GrandpaKeyChangePage({
  actorCidNumber,
  adminSigners,
  onBack,
  onSuccess,
}: Props) {
  // 正常轮换与紧急恢复共用一次管理员二维码签名流程，差异只由链端操作类型决定。
  const [changeKind, setChangeKind] = useState<GrandpaChangeKind>('routine_rotation');
  const [selectedSignerAccountId, setSelectedSignerAccountId] = useState(
    adminSigners.length === 1 ? adminSigners[0].account_id : '',
  );
  const [unlockPassword, setUnlockPassword] = useState('');
  const [step, setStep] = useState<FlowStep>('form');
  const [request, setRequest] = useState<GrandpaKeyChangeRequest | null>(null);
  const [countdown, setCountdown] = useState(90);
  const [txHash, setTxHash] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [building, setBuilding] = useState(false);
  const [status, setStatus] = useState<GrandpaKeyChangeStatus | null>(null);

  const requestRef = useRef(request);
  const signerRef = useRef(selectedSignerAccountId);
  requestRef.current = request;
  signerRef.current = selectedSignerAccountId;

  useEffect(() => {
    api.getStatus().then(setStatus).catch((cause) => setError(sanitizeError(cause)));
  }, []);

  useEffect(() => {
    if (step !== 'qr') return;
    if (countdown <= 0) {
      setError('管理员交易签名请求已过期；候选新私钥会在证明窗口过期后自动清理。');
      setStep('error');
      return;
    }
    const timer = window.setTimeout(() => setCountdown((value) => value - 1), 1000);
    return () => window.clearTimeout(timer);
  }, [step, countdown]);

  const buildRequest = useCallback(async () => {
    if (!selectedSignerAccountId || !unlockPassword) return;
    setBuilding(true);
    setError(null);
    try {
      const built = await api.buildRequest(
        actorCidNumber,
        selectedSignerAccountId,
        changeKind === 'emergency_recovery',
        unlockPassword,
      );
      setUnlockPassword('');
      setRequest(built);
      setCountdown(90);
      setStep('qr');
    } catch (cause) {
      setError(sanitizeError(cause));
      setStep('error');
    } finally {
      setBuilding(false);
    }
  }, [actorCidNumber, selectedSignerAccountId, changeKind, unlockPassword]);

  const submit = useCallback(async (responseJson: string) => {
    const currentRequest = requestRef.current;
    const signer = signerRef.current;
    if (!currentRequest || !signer) {
      setError('换钥签名请求数据丢失，请重新操作。');
      setStep('error');
      return;
    }
    setStep('submit');
    try {
      const result = await api.submit(
        currentRequest.requestId,
        signer,
        currentRequest.expectedPayloadHash,
        currentRequest.signNonce,
        currentRequest.signBlockNumber,
        responseJson,
      );
      setTxHash(result.txHash);
      setStep('done');
      setStatus(await api.getStatus());
    } catch (cause) {
      setError(sanitizeError(cause));
      setStep('error');
    }
  }, []);

  if (status?.pending && step === 'form') {
    return (
      <div className="governance-section">
        <button className="back-button" onClick={onBack}>← 返回</button>
        <h2>验证密钥</h2>
        <div className="warning">
          本机已有{kindLabel(status.changeKind)}流程，旧、新私钥正在同时保留。
          finalized 确认新 authority 生效后，节点会自动删除旧私钥并重启。
        </div>
        <div className="institution-detail-grid">
          <div className="metric-card">
            <div className="metric-label">旧 GRANDPA 公钥</div>
            <code>{status.oldPublicKey ?? '—'}</code>
          </div>
          <div className="metric-card">
            <div className="metric-label">新 GRANDPA 公钥</div>
            <code>{status.newPublicKey ?? '—'}</code>
          </div>
        </div>
        {status.txHash ? <code className="tx-hash">交易哈希: {status.txHash}</code> : (
          <p className="no-data">管理员交易尚未提交；请在当前签名会话中继续完成。</p>
        )}
      </div>
    );
  }

  return (
    <div className="governance-section">
      <button className="back-button" onClick={onBack}>← 返回</button>
      <h2>验证密钥</h2>
      <p className="upgrade-proposal-hint">
        正常更换由单个委员发起并要求旧、新 GRANDPA 私钥共同签名；旧私钥不可用时，
        才选择紧急恢复并进入本机构委员内部投票。这里不使用联合投票。
      </p>

      {step === 'form' && (
        <div className="create-proposal-form">
          {error && <div className="error">{error}</div>}
          <div className="wallet-form-field">
            <label>更换路径</label>
            <select
              value={changeKind}
              onChange={(event) => setChangeKind(event.target.value as GrandpaChangeKind)}
              disabled={building}
            >
              <option value="routine_rotation">正常更换：单个委员 + 旧/新私钥签名</option>
              <option value="emergency_recovery">紧急恢复：旧私钥不可用 + 本机构内部投票</option>
            </select>
          </div>

          {changeKind === 'emergency_recovery' && (
            <div className="warning">
              仅在旧私钥已经丢失或无法签名时使用。NRC 只由 NRC 委员内部投票；
              各 PRC 只由本 PRC 委员内部投票。
            </div>
          )}

          <div className="wallet-form-field">
            <label>发起委员</label>
            <select
              value={selectedSignerAccountId}
              onChange={(event) => setSelectedSignerAccountId(event.target.value)}
              disabled={adminSigners.length <= 1 || building}
            >
              {adminSigners.length !== 1 && <option value="">请选择…</option>}
              {adminSigners.map((admin) => (
                <option key={admin.account_id} value={admin.account_id}>
                  {accountIdToSs58(admin.account_id)}
                </option>
              ))}
            </select>
          </div>

          <div className="wallet-form-field">
            <label>本机解锁密码</label>
            <input
              type="password"
              value={unlockPassword}
              onChange={(event) => setUnlockPassword(event.target.value)}
              autoComplete="current-password"
              disabled={building}
            />
          </div>

          <button
            className="vote-signing-confirm"
            disabled={!selectedSignerAccountId || !unlockPassword || building}
            onClick={buildRequest}
          >
            {building ? '正在生成并安全保存候选密钥…' : '生成换钥签名请求'}
          </button>
        </div>
      )}

      {step === 'qr' && request && (
        <div className="vote-signing-body">
          <div className="warning">
            新私钥已写入本机 keystore，旧私钥仍保留。新公钥：{request.newPublicKey}
          </div>
          <CitizenSignaturePanel
            qrValue={request.requestJson}
            countdownSeconds={countdown}
            onScan={submit}
            onScanError={(message) => {
              setError(message);
              setStep('error');
            }}
          />
        </div>
      )}

      {step === 'submit' && <p className="qr-instruction">正在验签并提交换钥交易…</p>}

      {step === 'done' && (
        <div className="vote-success">
          <p>{changeKind === 'emergency_recovery' ? '紧急恢复提案已进入本机构内部投票。' : '正常更换已调度延迟生效。'}</p>
          {txHash && <code className="tx-hash">交易哈希: {txHash}</code>}
          <p>finalized 确认新 authority 后，本机将自动删除旧私钥并重启节点。</p>
          <button className="vote-signing-confirm" onClick={onSuccess}>完成</button>
        </div>
      )}

      {step === 'error' && (
        <div className="vote-signing-body">
          <div className="error">{error}</div>
          <button className="vote-signing-confirm" onClick={onBack}>返回机构详情</button>
        </div>
      )}
    </div>
  );
}
