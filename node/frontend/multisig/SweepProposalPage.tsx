// 填写并准备机构费用账户归集提案。

import { sanitizeError } from "../client/tauri";
import { accountIdToSs58 } from "../client/ss58";
import { CitizenSignaturePanel } from "../desktop/signing";
import { TransferProposalDialog, type TransferProposalStep } from "./TransferProposalDialog";
import { multisigTransferApi as api } from "./api";
import type { RoleSigner, VoteSignRequestResult } from "./types";
// 手续费划转提案页面：金额输入 + QR 签名流程。
import { useState, useRef, useEffect, useCallback } from 'react';

type Props = {
  actorCidNumber: string;
  institutionAccountId: string;
  mainAccountId: string;
  cidFullName: string;
  roleSigners: RoleSigner[];
  onBack: () => void;
  onSuccess: () => void;
};

export function SweepProposalPage({
  actorCidNumber, institutionAccountId, mainAccountId, cidFullName, roleSigners, onBack, onSuccess,
}: Props) {
  const [step, setStep] = useState<TransferProposalStep>('form');
  const [selectedSigner, setSelectedSigner] = useState<RoleSigner | null>(
    roleSigners.length === 1 ? roleSigners[0] : null
  );
  const [amountYuan, setAmountYuan] = useState('');
  const [formError, setFormError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const [requestJson, setRequestJson] = useState('');
  const [countdown, setCountdown] = useState(90);
  const [error, setError] = useState<string | null>(null);
  const [txHash, setTxHash] = useState<string | null>(null);

  const formValuesRef = useRef({ amountYuan: 0 });
  const signRequestRef = useRef<VoteSignRequestResult | null>(null);
  const selectedSignerRef = useRef(selectedSigner);
  selectedSignerRef.current = selectedSigner;

  useEffect(() => {
    if (step !== 'qr') return;
    if (countdown <= 0) {
      signRequestRef.current = null;
      setError('签名请求已过期');
      setStep('error');
      return;
    }
    const timer = setTimeout(() => setCountdown((c) => c - 1), 1000);
    return () => clearTimeout(timer);
  }, [step, countdown]);

  const handleSubmit = async () => {
    if (!selectedSigner) { setFormError('请选择人员岗位'); return; }
    const amount = parseFloat(amountYuan.replace(/,/g, ''));
    if (isNaN(amount) || amount <= 0) { setFormError('金额必须大于 0'); return; }
    setFormError(null);
    setSubmitting(true);

    try {
      formValuesRef.current = { amountYuan: amount };
      const result = await api.buildProposeSweepRequest(
        selectedSigner.accountId, actorCidNumber, selectedSigner.roleCode,institutionAccountId, amount,
      );
      signRequestRef.current = result;
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
    if (!req) return;
    signRequestRef.current = null;
    const wallet = selectedSignerRef.current;
    if (!wallet) { setError('数据丢失，请重试'); setStep('error'); return; }
    setStep('submit');
    try {
      const result = await api.submitProposeSweep(
        req.requestId, wallet.accountId, req.expectedPayloadHash,
        actorCidNumber, wallet.roleCode,institutionAccountId, formValuesRef.current.amountYuan,
        req.signNonce, req.signBlockNumber, responseText,
      );
      setTxHash(result.txHash);
      setStep('done');
    } catch (e) {
      setError(sanitizeError(e));
      setStep('error');
    }
  }, [actorCidNumber, institutionAccountId]);

  return (
    <TransferProposalDialog
      title="手续费划转提案"
      institutionName={cidFullName}
      step={step}
      generating={submitting}
      signDisabled={!selectedSigner || !amountYuan}
      onCancel={() => { signRequestRef.current = null; onBack(); }}
      onSign={handleSubmit}
      onComplete={onSuccess}
      onRetry={() => { setError(null); setStep('form'); }}
    >
      {step === 'form' && (
        <div className="create-proposal-form">
          {formError && <div className="error" role="alert">{formError}</div>}
          <div className="wallet-form-field">
            <label htmlFor="transfer-signer">发起岗位人员</label>
            <select id="transfer-signer"
              value={selectedSigner?selectedSigner.personCid+':'+selectedSigner.roleCode:''}
              onChange={(e) => setSelectedSigner(roleSigners.find((w) => w.personCid+':'+w.roleCode === e.target.value) || null)}
              disabled={roleSigners.length <= 1 || submitting}
            >
              {roleSigners.length === 0 && <option value="">无已授权岗位</option>}
              {roleSigners.length === 1 ? (
                <option value={roleSigners[0].personCid+':'+roleSigners[0].roleCode}>{roleSigners[0].roleName} · {accountIdToSs58(roleSigners[0].accountId)}</option>
              ) : (
                <>
                  <option value="">请选择…</option>
                  {roleSigners.map((w) => (
                    <option key={w.personCid+':'+w.roleCode} value={w.personCid+':'+w.roleCode}>{w.roleName} · {accountIdToSs58(w.accountId)}</option>
                  ))}
                </>
              )}
            </select>
          </div>
          <div className="wallet-form-field">
            <label htmlFor="sweep-source">转出地址（费用账户）</label>
            <input id="sweep-source" type="text" value={accountIdToSs58(institutionAccountId)} readOnly />
          </div>
          <div className="wallet-form-field">
            <label htmlFor="sweep-destination">收款地址（本机构主账户）</label>
            <input id="sweep-destination" type="text" value={accountIdToSs58(mainAccountId)} readOnly />
          </div>
          <div className="wallet-form-field">
            <label htmlFor="transfer-amount">划转金额（元）</label>
            <input id="transfer-amount"
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
            扣除划转本金和手续费后，费用账户至少保留 1.11 元。单次划转本金不超过扣除 1.11 元后可用余额的 80%。
          </p>
        </div>
      )}

      {step === 'qr' && (
        <div className="vote-signing-body">
          <CitizenSignaturePanel
            qrValue={requestJson}
            countdownSeconds={countdown}
            onScan={handleScanResult}
            onScanError={(e) => {
              if (!signRequestRef.current) return;
              signRequestRef.current = null;
              setError(e);
              setStep('error');
            }}
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
        </div>
      )}

      {step === 'error' && (
        <div className="vote-signing-body">
          <div className="error">{error}</div>
        </div>
      )}
    </TransferProposalDialog>
  );
}
