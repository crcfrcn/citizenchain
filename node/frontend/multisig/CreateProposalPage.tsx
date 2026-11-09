import { sanitizeError } from "../client/tauri";
import { accountIdToSs58, ADDRESS_INPUT_PLACEHOLDER } from "../client/ss58";
import { CitizenSignaturePanel } from "../desktop/signing";
import { TransferProposalDialog, type TransferProposalStep } from "./TransferProposalDialog";
import { AddressScanModal } from "../desktop/scanner";
import { multisigTransferApi as api } from "./api";
import type { AdminSignerMatch, VoteSignRequestResult } from "./types";
import scanIcon from "../../../icons/scan.svg?url";
// 创建转账提案页面：表单 + QR 签名流程。
import { useState, useRef, useEffect, useCallback } from 'react';

type Props = {
  cidNumber: string;
  cidFullName: string;
  title: string;
  sourceAccountLabel: string;
  institutionAccountId: string;
  proposerRoleCode: string;
  adminSigners: AdminSignerMatch[];
  onBack: () => void;
  onSuccess: () => void;
};

export function CreateMultisigTransferPage({
  cidNumber, cidFullName, title, sourceAccountLabel, institutionAccountId, proposerRoleCode, adminSigners, onBack, onSuccess,
}: Props) {
  const [step, setStep] = useState<TransferProposalStep>('form');

  // 表单
  const [selectedSigner, setSelectedSigner] = useState<AdminSignerMatch | null>(
    adminSigners.length === 1 ? adminSigners[0] : null
  );
  const [beneficiary, setBeneficiary] = useState('');
  const [amountYuan, setAmountYuan] = useState('');
  const [remark, setRemark] = useState('');
  const [formError, setFormError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [showAddressScan, setShowAddressScan] = useState(false);

  // QR 签名
  const [requestJson, setRequestJson] = useState('');
  const [countdown, setCountdown] = useState(90);
  const [error, setError] = useState<string | null>(null);
  const [txHash, setTxHash] = useState<string | null>(null);

  // 用 ref 持有最新值，避免摄像头回调中的闭包过期
  const formValuesRef = useRef({ beneficiary: '', amountYuan: 0, remark: '' });
  const signRequestRef = useRef<VoteSignRequestResult | null>(null);
  const selectedSignerRef = useRef(selectedSigner);
  selectedSignerRef.current = selectedSigner;


  useEffect(() => {
    if (step !== 'qr') return;
    if (countdown <= 0) {
      signRequestRef.current = null;
      setError('签名请求已过期，请重新操作');
      setStep('error');
      return;
    }
    const timer = setTimeout(() => setCountdown((c) => c - 1), 1000);
    return () => clearTimeout(timer);
  }, [step, countdown]);

  const validateForm = (): string | null => {
    if (!selectedSigner) return '请选择管理员账户';
    if (!beneficiary.trim()) return ADDRESS_INPUT_PLACEHOLDER;
    const amount = parseFloat(amountYuan.replace(/,/g, ''));
    if (isNaN(amount) || amount < 1.11) return '转账金额不能低于 1.11 元';
    const remarkBytes = new TextEncoder().encode(remark);
    if (remarkBytes.length > 256) return `备注超过 256 字节（当前 ${remarkBytes.length}）`;
    return null;
  };

  const handleSubmit = async () => {
    const err = validateForm();
    if (err) { setFormError(err); return; }
    setFormError(null);
    setSubmitting(true);

    try {
      const amount = parseFloat(amountYuan.replace(/,/g, ''));
      formValuesRef.current = { beneficiary: beneficiary.trim(), amountYuan: amount, remark };

      const result = await api.buildMultisigTransferRequest(
        selectedSigner!.accountId, cidNumber, proposerRoleCode, institutionAccountId,
        beneficiary.trim(), amount, remark,
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

  // 扫描结果处理（通过 ref 读取最新值）
  const handleScanResult = useCallback(async (responseText: string) => {
    const req = signRequestRef.current;
    if (!req) return;
    signRequestRef.current = null;
    const wallet = selectedSignerRef.current;
    if (!wallet) {
      setError('签名请求数据丢失，请重试');
      setStep('error');
      return;
    }
    setStep('submit');
    try {
      const { beneficiary: ben, amountYuan: amt, remark: rmk } = formValuesRef.current;
      const result = await api.submitMultisigTransfer(
        req.requestId, wallet.accountId, req.expectedPayloadHash,
        cidNumber, proposerRoleCode, institutionAccountId, ben, amt, rmk,
        req.signNonce, req.signBlockNumber, responseText,
      );
      setTxHash(result.txHash);
      setStep('done');
    } catch (e) {
      setError(sanitizeError(e));
      setStep('error');
    }
  }, [cidNumber, proposerRoleCode, institutionAccountId]);

  return (
    <TransferProposalDialog
      title={title}
      institutionName={cidFullName}
      step={step}
      generating={submitting}
      signDisabled={!selectedSigner || !beneficiary.trim() || !amountYuan}
      onCancel={() => { signRequestRef.current = null; onBack(); }}
      onSign={handleSubmit}
      onComplete={onSuccess}
      onRetry={() => { setError(null); setStep('form'); }}
      scanner={showAddressScan && (
        <AddressScanModal
          onResult={({ ss58Address }) => {
            // 账户码只声明账户，不带金额与备注；两者一律由本端手工填写。
            setBeneficiary(ss58Address);
            setShowAddressScan(false);
          }}
          onClose={() => setShowAddressScan(false)}
        />
      )}
    >
      {step === 'form' && (
        <div className="create-proposal-form">
          {formError && <div className="error" role="alert">{formError}</div>}

          <div className="wallet-form-field">
            <label htmlFor="transfer-signer">发起管理员</label>
            <select id="transfer-signer"
              value={selectedSigner?.accountId || ''}
              onChange={(e) => {
                const w = adminSigners.find((w) => w.accountId === e.target.value);
                setSelectedSigner(w || null);
              }}
              disabled={adminSigners.length <= 1 || submitting}
            >
              {adminSigners.length === 0 && <option value="">无已激活管理员</option>}
              {adminSigners.length === 1 ? (
                <option value={adminSigners[0].accountId}>{accountIdToSs58(adminSigners[0].accountId)}</option>
              ) : (
                <>
                  <option value="">请选择…</option>
                  {adminSigners.map((w) => (
                    <option key={w.accountId} value={w.accountId}>{accountIdToSs58(w.accountId)}</option>
                  ))}
                </>
              )}
            </select>
          </div>

          <div className="wallet-form-field">
            <label htmlFor="transfer-source">转出地址（{sourceAccountLabel}）</label>
            <input id="transfer-source" type="text" value={accountIdToSs58(institutionAccountId)} readOnly />
          </div>


          <div className="wallet-form-field">
            <label htmlFor="transfer-recipient">收款地址（SS58 或以太坊）</label>
            <div className="address-input-row">
              <input id="transfer-recipient"
                type="text" value={beneficiary}
                onChange={(e) => setBeneficiary(e.target.value)}
                placeholder={ADDRESS_INPUT_PLACEHOLDER}
                disabled={submitting}
              />
              <button type="button" className="scan-icon-btn" onClick={() => setShowAddressScan(true)} disabled={submitting} title="扫码填入">
                <span aria-hidden="true" style={{display:"inline-block",width:18,height:18,backgroundColor:"currentColor",maskImage:`url("${scanIcon}")`,WebkitMaskImage:`url("${scanIcon}")`,maskSize:"contain",WebkitMaskSize:"contain",maskRepeat:"no-repeat",WebkitMaskRepeat:"no-repeat"}} />
              </button>
            </div>
          </div>

          <div className="wallet-form-field">
            <label htmlFor="transfer-amount">转账金额（元，最少 1.11）</label>
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

          <div className="wallet-form-field">
            <label htmlFor="transfer-remark">备注（可选，最长 256 字节）</label>
            <input id="transfer-remark"
              type="text" value={remark}
              onChange={(e) => setRemark(e.target.value)}
              placeholder="转账备注" disabled={submitting}
            />
          </div>
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
        <div className="vote-signing-body"><p className="qr-instruction">正在提交提案到链…</p></div>
      )}

      {step === 'done' && (
        <div className="vote-signing-body">
          <div className="vote-success">
            <p>提案已提交</p>
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
