// 协调转账审阅、冷热签名、提交和执行结果展示。

import { signatureSecondsRemaining } from '../client/citizenQr';
import { accountIdToSs58 } from "../client/ss58";
import { sanitizeError } from "../client/tauri";
import { CitizenSignaturePanel } from "../desktop/signing";
import { transactionApi as api } from "./api";
import { formatFen, quoteTransfers } from "./fee";
import type { SignMode, TransferSignRequestResult, TransferSubmitResult, TransferDraft, Wallet } from "./types";
import { useState, useEffect, useRef, useCallback } from 'react';
type Props = {
  wallet: Wallet;
    executionFee?: boolean;
    mode: TransferDraft['mode'];
    transfers: TransferDraft['transfers'];
  onClose: () => void;
  // 提交成功返回交易哈希时通知一次，与用户如何关闭弹窗无关。
    onSuccess: (result: TransferSubmitResult) => void;
};
type FlowStep = 'confirm' | 'qr' | 'submit' | 'done' | 'error';
function signModeLabel(signMode: SignMode): string {
  switch (signMode) {
    case 'hot':
      return '热签（矿工钱包签名）';
    case 'cold':
      return '冷签（公民钱包离线签名）';
    default:
      throw new Error(`不支持的 signMode: ${String(signMode)}`);
  }
}
function confirmButtonLabel(signMode: SignMode): string {
  switch (signMode) {
    case 'hot':
      return '确认并提交';
    case 'cold':
      return '确认转账';
    default:
      throw new Error(`不支持的 signMode: ${String(signMode)}`);
  }
}
export function TransferSigningFlow({ wallet, executionFee = true, mode, transfers, onClose, onSuccess }: Props) {
  const [step, setStep] = useState<FlowStep>('confirm');
  const [signRequest, setSignRequest] = useState<TransferSignRequestResult | null>(null);
  const [requestJson, setRequestJson] = useState('');
  const [countdown, setCountdown] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [txHash, setTxHash] = useState<string | null>(null);
  const [unlockPassword, setUnlockPassword] = useState('');
  const [busy, setBusy] = useState(false);
  // 状态更新前也可能重复点击或扫码；提交成功后保持锁定至关闭，失败才允许重试。
  const inFlight = useRef(false);
  const signRequestRef = useRef(signRequest);
  signRequestRef.current = signRequest;
  const isHot = wallet.signMode === 'hot';
    const quote = quoteTransfers(transfers);
  useEffect(() => {
        if (step !== 'qr' || !requestJson)
            return;
    if (countdown <= 0) {
      setError('签名请求已过期，请重新操作');
      setStep('error');
      return;
    }
    const timer = setTimeout(() => setCountdown(signatureSecondsRemaining(requestJson)), 1000);
    return () => clearTimeout(timer);
  }, [step, countdown, requestJson]);
  const handleConfirm = useCallback(async () => {
        if (inFlight.current)
            return;
    inFlight.current = true;
    setBusy(true);
    setError(null);
    let submitted = false;
    try {
      switch (wallet.signMode) {
        case 'hot': {
          if (!unlockPassword.trim()) {
            setError('请输入设备开机密码');
            return;
          }
          setStep('submit');
                    const result = await api.submitHotTransfer(wallet.accountId, mode, transfers, unlockPassword);
          setUnlockPassword('');
          setTxHash(result.txHash);
          setStep('done');
          submitted = true;
                    onSuccess(result);
          return;
        }
        case 'cold': {
                    const result = await api.buildColdTransferRequest(wallet.accountId, mode, transfers);
          setSignRequest(result);
          setRequestJson(result.requestJson);
          setCountdown(signatureSecondsRemaining(result.requestJson));
          setStep('qr');
          return;
        }
        default:
          throw new Error(`不支持的 signMode: ${String(wallet.signMode)}`);
      }
        }
        catch (e) {
      setError(sanitizeError(e));
      setStep('error');
        }
        finally {
      inFlight.current = submitted;
      setBusy(false);
    }
    }, [wallet.accountId, wallet.signMode, mode, transfers, unlockPassword, onSuccess]);
  const handleScanResult = useCallback(async (responseText: string) => {
        if (inFlight.current)
            return;
    if (wallet.signMode !== 'cold') {
      setError('只有 Cold 钱包可以提交离线扫码签名');
      setStep('error');
      return;
    }
    const req = signRequestRef.current;
    if (!req) {
      setError('签名请求数据丢失，请重试');
      setStep('error');
      return;
    }
    inFlight.current = true;
    setStep('submit');
    try {
            const result = await api.submitColdTransfer(wallet.accountId, req.requestId, req.expectedPayloadHash, req.callDataHex, req.signNonce, req.signBlockNumber, responseText);
      setTxHash(result.txHash);
      setStep('done');
            onSuccess(result);
        }
        catch (e) {
      inFlight.current = false;
      setError(sanitizeError(e));
      setStep('error');
    }
  }, [wallet.accountId, wallet.signMode, onSuccess]);
    return (<div className="transfer-signing-overlay">
      <div className={`transfer-signing-modal ${step === 'qr' ? 'signature-flow-modal' : ''}`}>
        {/* 统一右上角关闭叉 */}
        <div className="transfer-signing-header">
          <h3>转账</h3>
          <span className="transfer-signing-close" onClick={onClose}>&times;</span>
        </div>

        {step === 'confirm' && (<div className="transfer-signing-body">
            <div className="transfer-signing-summary">
              <div className="transfer-signing-row">
                <span className="transfer-signing-label">付款地址</span>
                <span className="transfer-signing-value">{accountIdToSs58(wallet.accountId)}</span>
              </div>
              <div className="transfer-signing-row"><span className="transfer-signing-label">交易模式</span><span className="transfer-signing-value">{mode === 'batch_all' ? '整批成功' : mode === 'force_batch' ? '逐笔成功' : mode === 'batch' ? '遇错停止' : '单笔转账'}</span></div>
              {transfers.map((item, index) => <div className="transfer-signing-item" key={index}>
                {mode !== 'transfer' && <strong>第{index + 1}笔</strong>}
                <div className="transfer-signing-row"><span className="transfer-signing-label">收款地址</span><span className="transfer-signing-value">{item.toAddress}</span></div>
                {mode !== 'transfer' && <div>金额：{formatFen(item.amountFen)} 元</div>}
                {item.remark && <div className="transfer-signing-row"><span className="transfer-signing-label">备注</span><span className="transfer-signing-value">{item.remark}</span></div>}
              </div>)}
              <div className="transfer-signing-row"><span className="transfer-signing-label">转账金额</span><span className="transfer-signing-value">{formatFen(quote.amount)} 元</span></div>
              <div className="transfer-signing-row"><span className="transfer-signing-label">预估手续费</span><span className="transfer-signing-value">{formatFen(quote.fee)} 元（全部成功）</span></div>
              <div className="transfer-signing-row"><span className="transfer-signing-label">合计</span><span className="transfer-signing-value">{formatFen(quote.total)} 元</span></div>
              {(!executionFee || mode === 'batch_all') && <p>{!executionFee ? '当前Runtime按历史规则外层预扣，执行失败仍收费。' : '任一笔失败，整批本金、手续费和分账一起回滚。'}</p>}
              <div className="transfer-signing-row">
                <span className="transfer-signing-label">签名方式</span>
                <span className="transfer-signing-value">{signModeLabel(wallet.signMode)}</span>
              </div>
            </div>
            {isHot && (<div className="transfer-signing-password">
                <label>设备开机密码</label>
                <input type="password" value={unlockPassword} onChange={(e) => setUnlockPassword(e.target.value)} placeholder="请输入设备开机密码" autoFocus/>
              </div>)}
            {error && <div className="error">{error}</div>}
            <div className="transfer-signing-actions">
              <button className="transfer-signing-confirm" onClick={handleConfirm} disabled={busy || (isHot && !unlockPassword.trim())}>
                {confirmButtonLabel(wallet.signMode)}
              </button>
              <button className="cancel-button" onClick={onClose}>取消</button>
            </div>
          </div>)}

        {step === 'qr' && (<div className="transfer-signing-body">
            <CitizenSignaturePanel qrValue={requestJson} countdownSeconds={countdown} onScan={handleScanResult} onScanError={(e) => { setError(e); setStep('error'); }}/>
          </div>)}

        {step === 'submit' && (<div className="transfer-signing-body">
            <p className="qr-instruction">提交中...</p>
          </div>)}

        {step === 'done' && (<div className="transfer-signing-body">
            <div className="transfer-success">
              <p>转账已提交</p>
              {txHash && <code className="tx-hash">交易哈希: {txHash}</code>}
            </div>
            <button className="transfer-signing-confirm" onClick={onClose}>
              完成
            </button>
          </div>)}

        {step === 'error' && (<div className="transfer-signing-body">
            <div className="error">{error}</div>
            <button className="transfer-signing-confirm" onClick={() => { setError(null); setStep('confirm'); }}>
              重试
            </button>
          </div>)}
          </div>
    </div>);
}
