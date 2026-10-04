// 手续费划转提案页面：金额输入 + QR 签名流程。
import { useState, useRef, useEffect, useCallback } from 'react';
import { sanitizeError } from '../../tauri';
import { accountIdToSs58 } from '../../shared/ss58';
import { CitizenSignaturePanel } from '../../shared/qr/CitizenSignaturePanel';
import { multisigTransferApi as api } from './api';
import type { AdminSignerMatch, VoteSignRequestResult } from './types';

type Props = {
  actorCidNumber: string;
  institution_account_id: string;
  cidFullName: string;
  adminSigners: AdminSignerMatch[];
  onBack: () => void;
  onSuccess: () => void;
};

type Step = 'form' | 'qr' | 'submit' | 'done' | 'error';

export function SweepProposalPage({
  actorCidNumber, institution_account_id, cidFullName, adminSigners, onBack, onSuccess,
}: Props) {
  const [step, setStep] = useState<Step>('form');
  const [selectedSigner, setSelectedSigner] = useState<AdminSignerMatch | null>(
    adminSigners.length === 1 ? adminSigners[0] : null
  );
  const [amountYuan, setAmountYuan] = useState('');
  const [proposerRoleCode, setProposerRoleCode] = useState('');
  const [formError, setFormError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const [signRequest, setSignRequest] = useState<VoteSignRequestResult | null>(null);
  const [requestJson, setRequestJson] = useState('');
  const [countdown, setCountdown] = useState(90);
  const [error, setError] = useState<string | null>(null);
  const [txHash, setTxHash] = useState<string | null>(null);

  const formValuesRef = useRef({ amountYuan: 0 });
  const signRequestRef = useRef(signRequest);
  const selectedSignerRef = useRef(selectedSigner);
  signRequestRef.current = signRequest;
  selectedSignerRef.current = selectedSigner;

  useEffect(() => {
    if (step !== 'qr') return;
    if (countdown <= 0) { setError('签名请求已过期'); setStep('error'); return; }
    const timer = setTimeout(() => setCountdown((c) => c - 1), 1000);
    return () => clearTimeout(timer);
  }, [step, countdown]);

  const handleSubmit = async () => {
    if (!selectedSigner) { setFormError('请选择管理员账户'); return; }
    if (!proposerRoleCode.trim()) { setFormError('请输入提案发起岗位码'); return; }
    const amount = parseFloat(amountYuan.replace(/,/g, ''));
    if (isNaN(amount) || amount <= 0) { setFormError('金额必须大于 0'); return; }
    setFormError(null);
    setSubmitting(true);

    try {
      formValuesRef.current = { amountYuan: amount };
      const result = await api.buildProposeSweepRequest(
        selectedSigner.account_id, actorCidNumber, proposerRoleCode.trim(), institution_account_id, amount,
      );
      setSignRequest(result);
      setRequestJson(result.requestJson);
      setCountdown(90);
      setStep('qr');
    } catch (e) {
      setFormError(sanitizeError(e));
    } finally {
      setSubmitting(false);
    }
  };

  const handleScanResult = useCallback(async (responseText: string) => {
    const req = signRequestRef.current;
    const wallet = selectedSignerRef.current;
    if (!req || !wallet) { setError('数据丢失，请重试'); setStep('error'); return; }
    setStep('submit');
    try {
      const result = await api.submitProposeSweep(
        req.requestId, wallet.account_id, req.expectedPayloadHash,
        actorCidNumber, proposerRoleCode.trim(), institution_account_id, formValuesRef.current.amountYuan,
        req.signNonce, req.signBlockNumber, responseText,
      );
      setTxHash(result.txHash);
      setStep('done');
    } catch (e) {
      setError(sanitizeError(e));
      setStep('error');
    }
  }, [actorCidNumber, proposerRoleCode, institution_account_id]);

  return (
    <div className="governance-section">
      <button className="back-button" onClick={onBack}>← 返回</button>
      <h2>手续费划转提案</h2>
      <p className="proposal-institution-name">{cidFullName}</p>

      {step === 'form' && (
        <div className="create-proposal-form">
          {formError && <div className="error">{formError}</div>}
          <div className="wallet-form-field">
            <label>发起管理员</label>
            <select
              value={selectedSigner?.account_id || ''}
              onChange={(e) => setSelectedSigner(adminSigners.find((w) => w.account_id === e.target.value) || null)}
              disabled={adminSigners.length <= 1}
            >
              {adminSigners.length === 0 && <option value="">无已激活管理员</option>}
              {adminSigners.length === 1 ? (
                <option value={adminSigners[0].account_id}>{accountIdToSs58(adminSigners[0].account_id)}</option>
              ) : (
                <>
                  <option value="">请选择…</option>
                  {adminSigners.map((w) => (
                    <option key={w.account_id} value={w.account_id}>{accountIdToSs58(w.account_id)}</option>
                  ))}
                </>
              )}
            </select>
          </div>
          <div className="wallet-form-field">
            <label>转出地址（费用账户）</label>
            <input type="text" value={accountIdToSs58(institution_account_id)} disabled />
          </div>
          <div className="wallet-form-field">
            <label>提案发起岗位码</label>
            <input
              type="text" value={proposerRoleCode}
              onChange={(e) => setProposerRoleCode(e.target.value)}
              placeholder="NRC/PRC 填 COMMITTEE_MEMBER，PRB 填 DIRECTOR"
              maxLength={64} disabled={submitting}
            />
          </div>
          <div className="wallet-form-field">
            <label>划转金额（元）</label>
            <input
              type="text" inputMode="decimal" value={amountYuan}
              onChange={(e) => {
                const v = e.target.value.replace(/[^0-9.,]/g, '');
                const clean = v.replace(/,/g, '');
                const dot = clean.indexOf('.');
                const int = dot >= 0 ? clean.slice(0, dot) : clean;
                const dec = dot >= 0 ? clean.slice(dot) : '';
                setAmountYuan(int.replace(/\B(?=(\d{3})+(?!\d))/g, ',') + dec);
              }}
              placeholder="0.00"
              disabled={submitting}
            />
          </div>
          <p style={{ fontSize: 12, color: '#888', marginTop: 4 }}>
            划转后手续费账户至少保留 1111.11 元，单次不超过可用余额的 80%
          </p>
          <button
            className="vote-signing-confirm"
            onClick={handleSubmit}
            disabled={submitting || !selectedSigner || !amountYuan}
          >
            {submitting ? '生成中…' : '生成签名请求'}
          </button>
        </div>
      )}

      {step === 'qr' && (
        <div className="vote-signing-body">
          <CitizenSignaturePanel
            qrValue={requestJson}
            countdownSeconds={countdown}
            onScan={handleScanResult}
            onScanError={(e) => { setError(e); setStep('error'); }}
          />
        </div>
      )}

      {step === 'submit' && (
        <div className="vote-signing-body"><p className="qr-instruction">正在提交…</p></div>
      )}

      {step === 'done' && (
        <div className="vote-signing-body">
          <div className="vote-success">
            <p>手续费划转提案已提交</p>
            {txHash && <code className="tx-hash">交易哈希: {txHash}</code>}
          </div>
          <button className="vote-signing-confirm" onClick={onSuccess}>完成</button>
        </div>
      )}

      {step === 'error' && (
        <div className="vote-signing-body">
          <div className="error">{error}</div>
          <button className="vote-signing-confirm" onClick={() => { setError(null); setStep('form'); }}>重试</button>
        </div>
      )}
    </div>
  );
}
