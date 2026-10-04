// 运行期协议升级页：提交 WASM 提案,走联合投票流程。
import { useState, useEffect, useRef, useCallback } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { sanitizeError } from '../../tauri';
import { accountIdToSs58 } from '../../shared/ss58';
import { CitizenSignaturePanel } from '../../shared/qr/CitizenSignaturePanel';
import { runtimeUpgradeApi as api } from './api';
import type { AdminSignerMatch } from '../types';
import type { PowDifficultyParams, ProposeUpgradeRequestResult } from './api';

type FlowStep = 'form' | 'qr' | 'submit' | 'done' | 'error';

type Props = {
  actorCidNumber: string;
  adminSigners: AdminSignerMatch[];
  onBack: () => void;
  onSuccess: () => void;
};

export function ProtocolUpgradeProposalPage({ actorCidNumber, adminSigners, onBack, onSuccess }: Props) {
  const [wasmPath, setWasmPath] = useState('');
  const [wasmFileName, setWasmFileName] = useState('');
  const [reason, setReason] = useState('');
  const [selectedSignerAccountId, setSelectedSignerAccountId] = useState(
    adminSigners.length === 1 ? adminSigners[0].account_id : ''
  );
  const [step, setStep] = useState<FlowStep>('form');
  const [signRequest, setSignRequest] = useState<ProposeUpgradeRequestResult | null>(null);
  const [requestJson, setRequestJson] = useState('');
  const [countdown, setCountdown] = useState(90);
  const [error, setError] = useState<string | null>(null);
  const [txHash, setTxHash] = useState<string | null>(null);
  const [building, setBuilding] = useState(false);
  const [powParams, setPowParams] = useState<PowDifficultyParams | null>(null);

  const signRequestRef = useRef(signRequest);
  const selectedSignerAccountIdRef = useRef(selectedSignerAccountId);
  const wasmPathRef = useRef(wasmPath);
  const reasonRef = useRef(reason);
  const powParamsRef = useRef(powParams);
  signRequestRef.current = signRequest;
  selectedSignerAccountIdRef.current = selectedSignerAccountId;
  wasmPathRef.current = wasmPath;
  reasonRef.current = reason;
  powParamsRef.current = powParams;

  useEffect(() => {
    api.getPowDifficultyParams()
      .then(setPowParams)
      .catch((e) => setError(sanitizeError(e)));
  }, []);

  useEffect(() => {
    if (step !== 'qr') return;
    if (countdown <= 0) { setError('签名请求已过期，请重新操作'); setStep('error'); return; }
    const timer = setTimeout(() => setCountdown((c) => c - 1), 1000);
    return () => clearTimeout(timer);
  }, [step, countdown]);

  const handlePickFile = useCallback(async () => {
    try {
      const selected = await open({
        title: '选择 runtime WASM 文件',
        filters: [{ name: 'WASM', extensions: ['wasm'] }],
        multiple: false,
        directory: false,
      });
      if (selected) {
        setWasmPath(selected);
        const parts = selected.replace(/\\/g, '/').split('/');
        setWasmFileName(parts[parts.length - 1] || selected);
      }
    } catch (e) {
      setError(sanitizeError(e));
    }
  }, []);

  const handleBuildRequest = useCallback(async () => {
    if (!wasmPath.trim() || !selectedSignerAccountId || !reason.trim() || !powParams) return;
    setBuilding(true);
    setError(null);
    try {
      const result = await api.buildProposeUpgradeRequest(
        selectedSignerAccountId,
        actorCidNumber,
        wasmPath.trim(),
        reason.trim(),
        powParams,
      );
      setSignRequest(result);
      setRequestJson(result.requestJson);
      setCountdown(90);
      setStep('qr');
    } catch (e) {
      setError(sanitizeError(e));
      setStep('error');
    } finally {
      setBuilding(false);
    }
  }, [actorCidNumber, wasmPath, selectedSignerAccountId, reason, powParams]);

  const handleScanResult = useCallback(async (responseText: string) => {
    const req = signRequestRef.current;
    const signerAccountId = selectedSignerAccountIdRef.current;
    const path = wasmPathRef.current;
    const reasonVal = reasonRef.current;
    const params = powParamsRef.current;
    if (!req || !signerAccountId || !params) { setError('签名请求数据丢失，请重试'); setStep('error'); return; }
    setStep('submit');
    try {
      const result = await api.submitProposeUpgrade(
        req.requestId, signerAccountId, req.expectedPayloadHash,
        actorCidNumber,
        path, reasonVal, params,
        req.signNonce, req.signBlockNumber, responseText,
      );
      setTxHash(result.txHash);
      setStep('done');
    } catch (e) {
      setError(sanitizeError(e));
      setStep('error');
    }
  }, [actorCidNumber]);

  const canSubmit = wasmPath.trim() && selectedSignerAccountId && reason.trim() && powParams;

  return (
    <div className="governance-section">
      <button className="back-button" onClick={onBack}>&larr; 返回</button>
      <h2>协议升级</h2>
      <p className="upgrade-proposal-hint">
        提交运行期协议升级提案，进入联合投票流程。
      </p>

      {step === 'form' && (
        <div className="create-proposal-form">
          {error && <div className="error">{error}</div>}

          <div className="wallet-form-field">
            <label>升级理由</label>
            <textarea
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder="请输入本次升级的理由说明"
              rows={6}
              maxLength={1024}
              disabled={building}
              style={{ minHeight: '120px', resize: 'vertical' }}
            />
          </div>

          <div className="wallet-form-field">
            <label>Runtime WASM 文件</label>
            <div className="upgrade-file-picker">
              <button className="upgrade-file-button" onClick={handlePickFile} disabled={building}>
                选择文件
              </button>
              <span className="upgrade-file-name">
                {wasmFileName || '未选择文件'}
              </span>
            </div>
          </div>

          {powParams && (
            <div className="wallet-form-field">
              <label>PoW 参数（与 runtime code 一起进入联合投票）</label>
              {([
                ['paramsVersion', '参数版本'],
                ['algorithmVersion', '算法版本'],
                ['targetBlockTimeMs', '平均目标时间（毫秒）'],
                ['adjustmentInterval', '调整窗口（块）'],
                ['maxAdjustUpFactor', '最大上调倍率'],
                ['maxAdjustDownDivisor', '最大下调分母'],
              ] as const).map(([field, label]) => (
                <label key={field}>{label}
                  <input
                    type="number"
                    min={1}
                    value={powParams[field]}
                    onChange={(e) => setPowParams({
                      ...powParams,
                      [field]: Number(e.target.value),
                    })}
                    disabled={building}
                  />
                </label>
              ))}
            </div>
          )}

          <div className="wallet-form-field">
            <label>发起管理员</label>
            {adminSigners.length === 0 ? (
              <p className="upgrade-no-wallet">无已激活管理员</p>
            ) : (
              <select
                value={selectedSignerAccountId}
                onChange={(e) => setSelectedSignerAccountId(e.target.value)}
                disabled={adminSigners.length <= 1}
              >
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
            )}
          </div>

          <button
            className="vote-signing-confirm"
            disabled={!canSubmit || building}
            onClick={handleBuildRequest}
          >
            {building ? '正在生成协议升级签名请求…' : '生成协议升级签名请求'}
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
        <div className="vote-signing-body">
          <p className="qr-instruction">正在验证签名并提交升级提案…</p>
        </div>
      )}

      {step === 'done' && (
        <div className="vote-signing-body">
          <div className="vote-success">
            <p>协议升级提案已提交</p>
            {txHash && <code className="tx-hash">交易哈希: {txHash}</code>}
          </div>
          <p className="upgrade-done-note">提案已进入联合投票阶段。</p>
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
