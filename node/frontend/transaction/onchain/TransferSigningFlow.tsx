import { useState, useEffect, useRef, useCallback } from 'react';
import { sanitizeError } from '../../tauri';
import { CitizenSignaturePanel } from '../../shared/qr/CitizenSignaturePanel';
import { transactionApi as api } from './api';
import { calculateTransferFeeYuan } from './fee';
import type { SignMode, TransferSignRequestResult, Wallet } from './types';

type Props = {
  wallet: Wallet;
  toAddress: string;
  amountYuan: number;
  remark: string;
  onClose: () => void;
  onSuccess: (txHash: string) => void;
};

type FlowStep = 'confirm' | 'qr' | 'submit' | 'done' | 'error';

function truncateAddress(addr: string): string {
  if (addr.length <= 14) return addr;
  return addr.slice(0, 8) + '...' + addr.slice(-6);
}

function fmtYuan(v: number): string {
  const fixed = v.toFixed(2);
  const [int, dec] = fixed.split('.');
  return `${int.replace(/\B(?=(\d{3})+(?!\d))/g, ',')}.${dec}`;
}

function signModeLabel(signMode: SignMode): string {
  switch (signMode) {
    case 'hot':
      return 'Hot（本机签名）';
    case 'cold':
      return 'Cold（公民钱包离线签名）';
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

export function TransferSigningFlow({ wallet, toAddress, amountYuan, remark, onClose, onSuccess }: Props) {
  const [step, setStep] = useState<FlowStep>('confirm');
  const [signRequest, setSignRequest] = useState<TransferSignRequestResult | null>(null);
  const [requestJson, setRequestJson] = useState('');
  const [countdown, setCountdown] = useState(90);
  const [error, setError] = useState<string | null>(null);
  const [txHash, setTxHash] = useState<string | null>(null);
  const [unlockPassword, setUnlockPassword] = useState('');

  const signRequestRef = useRef(signRequest);
  signRequestRef.current = signRequest;

  const isHot = wallet.signMode === 'hot';
  const fee = calculateTransferFeeYuan(amountYuan);

  useEffect(() => {
    if (step !== 'qr') return;
    if (countdown <= 0) {
      setError('签名请求已过期，请重新操作');
      setStep('error');
      return;
    }
    const timer = setTimeout(() => setCountdown((c) => c - 1), 1000);
    return () => clearTimeout(timer);
  }, [step, countdown]);

  const handleConfirm = useCallback(async () => {
    setError(null);
    try {
      switch (wallet.signMode) {
        case 'hot': {
          if (!unlockPassword.trim()) {
            setError('请输入设备开机密码');
            return;
          }
          setStep('submit');
          const result = await api.submitHotTransfer(
            wallet.accountId,
            toAddress,
            amountYuan,
            remark,
            unlockPassword,
          );
          setUnlockPassword('');
          setTxHash(result.txHash);
          setStep('done');
          return;
        }
        case 'cold': {
          const result = await api.buildColdTransferRequest(
            wallet.accountId,
            toAddress,
            amountYuan,
            remark,
          );
          setSignRequest(result);
          setRequestJson(result.requestJson);
          setCountdown(90);
          setStep('qr');
          return;
        }
        default:
          throw new Error(`不支持的 signMode: ${String(wallet.signMode)}`);
      }
    } catch (e) {
      setError(sanitizeError(e));
      setStep('error');
    }
  }, [wallet.accountId, wallet.signMode, toAddress, amountYuan, remark, unlockPassword]);

  const handleScanResult = useCallback(async (responseText: string) => {
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
    setStep('submit');
    try {
      const result = await api.submitColdTransfer(
        wallet.accountId,
        req.requestId,
        req.expectedPayloadHash,
        req.callDataHex,
        req.signNonce,
        req.signBlockNumber,
        responseText,
      );
      setTxHash(result.txHash);
      setStep('done');
    } catch (e) {
      setError(sanitizeError(e));
      setStep('error');
    }
  }, [wallet.accountId, wallet.signMode]);

  return (
    <div className="transfer-signing-overlay">
      <div className={`transfer-signing-modal ${step === 'qr' ? 'signature-flow-modal' : ''}`}>
        {/* 统一右上角关闭叉 */}
        <div className="transfer-signing-header">
          <h3>转账</h3>
          <span className="transfer-signing-close" onClick={onClose}>&times;</span>
        </div>

        {step === 'confirm' && (
          <div className="transfer-signing-body">
            <div className="transfer-signing-summary">
              <div className="transfer-signing-row">
                <span className="transfer-signing-label">付款地址</span>
                <span className="transfer-signing-value">{wallet.ss58Address}</span>
              </div>
              <div className="transfer-signing-row">
                <span className="transfer-signing-label">收款地址</span>
                <span className="transfer-signing-value">{toAddress}</span>
              </div>
              <div className="transfer-signing-row">
                <span className="transfer-signing-label">转账金额</span>
                <span className="transfer-signing-value">{fmtYuan(amountYuan)} 元</span>
              </div>
              {remark.length > 0 && (
                <div className="transfer-signing-row">
                  <span className="transfer-signing-label">转账备注</span>
                  <span className="transfer-signing-value">{remark}</span>
                </div>
              )}
              <div className="transfer-signing-row">
                <span className="transfer-signing-label">预估手续费</span>
                <span className="transfer-signing-value">{fmtYuan(fee)} 元</span>
              </div>
              <div className="transfer-signing-row">
                <span className="transfer-signing-label">签名方式</span>
                <span className="transfer-signing-value">{signModeLabel(wallet.signMode)}</span>
              </div>
            </div>
            {isHot && (
              <div className="transfer-signing-password">
                <label>设备开机密码</label>
                <input
                  type="password"
                  value={unlockPassword}
                  onChange={(e) => setUnlockPassword(e.target.value)}
                  placeholder="请输入设备开机密码"
                  autoFocus
                />
              </div>
            )}
            {error && <div className="error">{error}</div>}
            <div className="transfer-signing-actions">
              <button
                className="transfer-signing-confirm"
                onClick={handleConfirm}
                disabled={isHot && !unlockPassword.trim()}
              >
                {confirmButtonLabel(wallet.signMode)}
              </button>
              <button className="cancel-button" onClick={onClose}>取消</button>
            </div>
          </div>
        )}

        {step === 'qr' && (
          <div className="transfer-signing-body">
            <CitizenSignaturePanel
              qrValue={requestJson}
              countdownSeconds={countdown}
              onScan={handleScanResult}
              onScanError={(e) => { setError(e); setStep('error'); }}
            />
          </div>
        )}

        {step === 'submit' && (
          <div className="transfer-signing-body">
            <p className="qr-instruction">提交中...</p>
          </div>
        )}

        {step === 'done' && (
          <div className="transfer-signing-body">
            <div className="transfer-success">
              <p>转账已提交</p>
              {txHash && <code className="tx-hash">交易哈希: {txHash}</code>}
            </div>
            <button
              className="transfer-signing-confirm"
              onClick={() => { if (txHash) onSuccess(txHash); onClose(); }}
            >
              完成
            </button>
          </div>
        )}

        {step === 'error' && (
          <div className="transfer-signing-body">
            <div className="error">{error}</div>
            <button
              className="transfer-signing-confirm"
              onClick={() => { setError(null); setStep('confirm'); }}
            >
              重试
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
