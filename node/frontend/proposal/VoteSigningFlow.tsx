// 协调提案投票审阅、冷签响应及交易提交。

import type { FrozenVoteBinding } from "./types";
import { parseQrEnvelope, type SignResponseBody } from "../client/citizenQr";
import { proposalApi } from './api';
import { accountIdToSs58 } from "../client/ss58";
import { sanitizeError } from "../client/tauri";
import { CitizenSignaturePanel } from "../desktop/signing";
import type { RoleSigner } from "../institution/council";
import type { VoteSignRequestResult } from "../client/signing";

// 投票签名流程：选钱包 → 显示 QR → 摄像头扫描响应 → 提交。
import { useState, useEffect, useRef, useCallback } from 'react';

type PendingFrozenVote = { binding: FrozenVoteBinding; request: VoteSignRequestResult; response: string; approve: boolean; txHash?: string | null };
const pendingKey = (binding: FrozenVoteBinding) => 'proposal-vote:' + JSON.stringify([binding.genesisHash, binding.proposalId, binding.cidNumber, binding.roleCode, binding.personCid]);
export function readPendingFrozenVote(binding: FrozenVoteBinding): PendingFrozenVote | null {
  const raw = window.localStorage.getItem(pendingKey(binding));
  if (!raw) return null;
  const value = JSON.parse(raw) as PendingFrozenVote;
  if (!value.binding?.personCid || !/^[1-9][0-9]*$/.test(value.binding.bindingRevision) || 'walletRevision' in value.binding || pendingKey(value.binding) !== pendingKey(binding) || typeof value.response !== 'string' ||
      typeof value.approve !== 'boolean' || !value.request?.requestId) throw new Error('原签名记录损坏，请保留记录核查');
  return value;
}
export function clearPendingFrozenVote(binding: FrozenVoteBinding) { window.localStorage.removeItem(pendingKey(binding)); }
function savePending(value: PendingFrozenVote) { window.localStorage.setItem(pendingKey(value.binding), JSON.stringify(value)); }

type Props = {
  proposalId: string; proposalKind: number; roleSigners: RoleSigner[]; cidNumber?: string;
  frozenTicket?: FrozenVoteBinding;
  onClose: () => void;
  onSuccess: (txHash: string | null, accountId: string, voterRoleCode: string | null, observed?: boolean) => void;
};
type FlowStep = 'select' | 'qr' | 'submit' | 'done' | 'error';

export function VoteSigningFlow({ proposalId, proposalKind, roleSigners, cidNumber, frozenTicket, onClose, onSuccess }: Props) {
  const [step, setStep] = useState<FlowStep>('select');
  const [selectedSigner, setSelectedSigner] = useState<RoleSigner | null>(roleSigners.length === 1 ? roleSigners[0] : null);
  const [approve, setApprove] = useState<boolean | null>(null);
  const [selectedRoleCode, setSelectedRoleCode] = useState(frozenTicket?.roleCode ??
    (roleSigners.length===1?roleSigners[0].roleCode:''));
  const [signRequest, setSignRequest] = useState<VoteSignRequestResult | null>(null);
  const [requestJson, setRequestJson] = useState('');
  const [countdown, setCountdown] = useState(90);
  const [error, setError] = useState<string | null>(null);
  const [txHash, setTxHash] = useState<string | null>(null);
  const [observed, setObserved] = useState(false);
  const [pending, setPending] = useState<PendingFrozenVote | null>(null);
  const busy = useRef(false);
  const active = useRef(false);
  const [recordError, setRecordError] = useState(false);
  const epoch = useRef(0);
  const context = JSON.stringify([proposalId, proposalKind, cidNumber, frozenTicket, roleSigners]);
  const liveContext = useRef(context); liveContext.current = context;
  useEffect(() => {
    active.current = true; setRecordError(false);
    ++epoch.current; busy.current = false;
    setStep('select'); setSignRequest(null); setError(null); setPending(null);
    setSelectedSigner(roleSigners.length === 1 ? roleSigners[0] : null);
    setSelectedRoleCode(frozenTicket?.roleCode ?? (roleSigners.length===1?roleSigners[0].roleCode:''));
    setApprove(null);
    try {
      const saved = frozenTicket ? readPendingFrozenVote(frozenTicket) : null;
      if (saved) { setPending(saved); setStep('error'); setError('存在原签名记录，请恢复原会话核实链上状态'); }
    } catch (e) { setRecordError(true); setError(sanitizeError(e)); setStep('error'); }
    return () => { active.current = false; ++epoch.current; };
  }, [context]);
  const close = () => { active.current = false; ++epoch.current; onClose(); };
  useEffect(() => {
    if (step !== 'qr') return;
    if (countdown <= 0) { setError('签名请求已过期'); setStep('error'); return; }
    const timer = setTimeout(() => setCountdown(c => c - 1), 1000);
    return () => clearTimeout(timer);
  }, [step, countdown]);

  const generateRequest = useCallback(async () => {
    if (!active.current || context !== liveContext.current || recordError || busy.current || pending || !selectedSigner || approve === null || (!!cidNumber && !selectedRoleCode)) return;
    busy.current = true; const token = epoch.current;
    try {
      const result = proposalKind === 1 && cidNumber ?
        await proposalApi.buildJointVoteRequest(proposalId, selectedSigner.accountId, cidNumber, selectedRoleCode, approve) :
        await proposalApi.buildVoteRequest(proposalId, selectedSigner.accountId, cidNumber ? selectedRoleCode : null, approve);
      if (token !== epoch.current) return;
      setSignRequest(result); setRequestJson(result.requestJson); setCountdown(90); setStep('qr');
    } catch (e) { if (token === epoch.current) { setError(sanitizeError(e)); setStep('error'); } }
    finally { if (token === epoch.current) busy.current = false; }
  }, [proposalId, proposalKind, selectedSigner, selectedRoleCode, approve, cidNumber, pending, context, recordError]);

  const submit = async (value: PendingFrozenVote | null, req: VoteSignRequestResult, response: string, wallet: string) => {
    if (!active.current || context !== liveContext.current || busy.current) return;
    busy.current = true; const token = epoch.current; setStep('submit');
    try {
      // 先持久保留原签名，再进入可能丢失响应的广播；恢复绝不生成新nonce。
      if (value) { setPending(value); savePending(value); }
      const result = await proposalApi.submitVote(req.requestId, wallet, req.expectedPayloadHash, req.callDataHex,
        req.signNonce, req.signBlockNumber, response, value?.binding);
      if (value) {
        if (result.observed) clearPendingFrozenVote(value.binding);
        else savePending({ ...value, txHash: result.txHash });
      }
      if (token !== epoch.current) return;
      setTxHash(result.txHash); setObserved(result.observed); setStep('done');
      onSuccess(result.txHash, wallet, cidNumber ? selectedRoleCode : null, result.observed);
    } catch (e) { if (token === epoch.current) { setError(sanitizeError(e)); setStep('error'); } }
    finally { if (token === epoch.current) busy.current = false; }
  };
  const handleScanResult = async (responseText: string) => {
    if (!active.current || context !== liveContext.current || busy.current || !signRequest || !selectedSigner || approve === null) return;
    try {
      if (frozenTicket) {
        const response = parseQrEnvelope(responseText);
        const request = parseQrEnvelope(signRequest.requestJson);
        if (response.kind !== 'sign_response' || response.id !== signRequest.requestId ||
            response.expires_at !== request.expires_at ||
            (response.body as SignResponseBody).signer_public_key !== frozenTicket.accountId) throw new Error('响应不属于原钱包或原签名请求');
      }
      const value = frozenTicket ? { binding: frozenTicket, request: signRequest, response: responseText, approve } : null;
      await submit(value, signRequest, responseText, selectedSigner.accountId);
    } catch (e) { setError(sanitizeError(e)); setStep('error'); }
  };
  return (
    <div className="vote-signing-overlay">
      <div className={`vote-signing-modal ${step === 'qr' ? 'signature-flow-modal' : ''}`}>
        <div className="vote-signing-header">
          <h3>{proposalKind === 1 ? '联合投票' : '投票'}</h3>
          <button className="vote-signing-close" onClick={close}>✕</button>
        </div>

        {step === 'select' && (
          <div className="vote-signing-body">
            {frozenTicket && <p>本次签名钱包：{accountIdToSs58(frozenTicket.accountId)} / 原岗位：{frozenTicket.roleCode} / 当前绑定版本：{frozenTicket.bindingRevision}。须使用此钱包签名。</p>}
            {roleSigners.length > 1 && !frozenTicket && (
              <div className="vote-signing-field">
                <label>选择人员岗位</label>
                <select value={selectedSigner?selectedSigner.personCid+':'+selectedSigner.roleCode:''} onChange={(e) => {
                  const signer = roleSigners.find((item) => item.personCid+':'+item.roleCode === e.target.value) || null;
                  setSelectedSigner(signer);
                  setSelectedRoleCode(signer?.roleCode??'');
                }}>
                  <option value="">请选择…</option>
                  {roleSigners.map((signer) => (
                    <option key={signer.personCid+':'+signer.roleCode} value={signer.personCid+':'+signer.roleCode}>
                      {signer.roleName || accountIdToSs58(signer.accountId)}
                    </option>
                  ))}
                </select>
              </div>
            )}
            {cidNumber && selectedSigner && !frozenTicket && (
              <div className="vote-signing-field">
                <label>使用岗位</label>
                <select value={selectedRoleCode} onChange={(e) => setSelectedRoleCode(e.target.value)}>
                  <option value="">请选择…</option>
                  {[selectedSigner].map((assignment) => (
                    <option key={assignment.roleCode} value={assignment.roleCode}>
                      {assignment.roleName || assignment.roleCode}（{assignment.roleCode}）
                    </option>
                  ))}
                </select>
              </div>
            )}
            <div className="vote-signing-field">
              <label>投票方向</label>
              <div className="vote-direction-buttons">
                <button className={`vote-dir-btn ${approve === true ? 'selected-yes' : ''}`} onClick={() => setApprove(true)}>赞成</button>
                <button className={`vote-dir-btn ${approve === false ? 'selected-no' : ''}`} onClick={() => setApprove(false)}>反对</button>
              </div>
            </div>
            <button className="vote-signing-confirm" disabled={!selectedSigner || approve === null || (!!cidNumber && !selectedRoleCode)} onClick={generateRequest}>生成签名请求</button>
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

        {step === 'submit' && <div className="vote-signing-body"><p className="qr-instruction">正在验证签名并提交到链…</p></div>}

        {step === 'done' && (
          <div className="vote-signing-body">
            <div className="vote-success"><p>{observed ? '链上已记录该票' : '已广播，等待链上票据确认'}</p>{txHash && <code className="tx-hash">交易哈希: {txHash}</code>}</div>
            <button className="vote-signing-confirm" onClick={close}>完成</button>
          </div>
        )}

        {step === 'error' && (
          <div className="vote-signing-body">
            <div className="error">{error}</div>
            {recordError ? <p>保留原记录核查，不能重新准备签名。</p> : pending ? <button className="vote-signing-confirm" onClick={() => void submit(pending, pending.request, pending.response, pending.binding.accountId)}>恢复原会话</button> :
              <button className="vote-signing-confirm" onClick={() => { setError(null); setStep('select'); }}>返回选择</button>}
          </div>
        )}
      </div>
    </div>
  );
}
