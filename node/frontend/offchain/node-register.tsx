// 填写清算行节点资料，检测连通性并冷签登记。

import { IdentityCidText } from '../client/identityCid';
import { sanitizeError } from "../client/tauri";
import { roleSigners as selectedRoleSigners } from "../institution/assignments";
import { rolesApi } from "../institution/roles";
import { accountIdToSs58 } from "../client/ss58";
import { CitizenSignatureModal } from "../desktop/signing";
import type { RoleSigner } from "../institution/council";
import type { VoteSignRequestResult } from "../client/signing";
import { offchainApi } from "./api";
import type { ConnectivityTestReport } from "./types";
// 声明清算行节点页:peer_id(自动)+ rpc_domain/port(手填)+ 4 重连通性自测 + QR 签名提交。
//
// 校验序列:
//   1. 用户填 RPC 域名 + 端口
//   2. 点"自测连通性",节点桌面端跑 DNS/wss/链ID/PeerId 4 重检查
//   3. 全部通过才解锁"扫码签名提交"按钮
//   4. 公民钱包扫请求 QR → 摄像头扫响应 QR → 链上 register_clearing_bank
//
// 注:peer_id 由本机 system_localPeerId RPC 拉,用户不可手改(防输错;同时与连通性
//    自测的远端校验形成闭环——远端必须返回与本字段相同的 PeerId 才放行)。

import { useCallback, useEffect, useRef, useState } from 'react';

type Props = {
  cidNumber: string;
  cidFullName: string;
  onBack: () => void;
  onSuccess: () => void;
};

type Step = 'form' | 'testing' | 'tested' | 'qr' | 'submit' | 'done' | 'error';

export function ClearingBankDeclareNodePage({ cidNumber, cidFullName, onBack, onSuccess }: Props) {
  const [step, setStep] = useState<Step>('form');
  const [error, setError] = useState<string | null>(null);

  // 表单
  const [peerId, setPeerId] = useState('');
  const [rpcDomain, setRpcDomain] = useState('');
  const [rpcPort, setRpcPort] = useState<string>('9944');
  const [report, setReport] = useState<ConnectivityTestReport | null>(null);

  // 按人员与岗位选择激活记录；同一钱包的不同岗位分别展示。
  const [signers, setSigners] = useState<RoleSigner[]>([]);
  const [selectedSigner, setSelectedSigner] = useState<RoleSigner | null>(null);

  // QR 流程
  const [signRequest, setSignRequest] = useState<VoteSignRequestResult | null>(null);
  const [countdown, setCountdown] = useState(90);
  const [txHash, setTxHash] = useState<string | null>(null);

  const signRequestRef = useRef(signRequest);
  signRequestRef.current = signRequest;

  // 初始化：读取本机 PeerId、现行岗位及匹配的激活记录。
  useEffect(() => {
    let cancelled = false;
    setSigners([]);
    setSelectedSigner(null);
    Promise.all([
      offchainApi.queryLocalPeerId().catch(() => ''),
      rolesApi.getRoleAccess(cidNumber, { cidNumber }),
      rolesApi.getInstitutionRoles({cidNumber}),
    ]).then(([pid, aa, state]) => {
      if (cancelled) return;
      setPeerId(pid);
      const matches=selectedRoleSigners(state?.roles??[],aa).filter(s=>s.permissions.some(p=>p.moduleTag==='offchain'&&p.actionCode===50&&p.operation===0));
      if (state?.cidNumber !== cidNumber) throw new Error('机构岗位响应不匹配');
      setSigners(matches);
      if (matches.length === 1) setSelectedSigner(matches[0]);
    }).catch(e => { if (!cancelled) setError(sanitizeError(e)); });
    return () => { cancelled = true; };
  }, [cidNumber]);

  useEffect(() => {
    if (step !== 'qr') return;
    if (countdown <= 0) {
      setError('签名请求已过期,请重新生成');
      setStep('error');
      return;
    }
    const t = setTimeout(() => setCountdown(c => c - 1), 1000);
    return () => clearTimeout(t);
  }, [step, countdown]);

  const runConnectivityTest = useCallback(async () => {
    setError(null);
    if (!peerId) {
      setError('本机 PeerId 尚未取到,请确认节点正在运行');
      return;
    }
    if (!rpcDomain.trim()) {
      setError('请填写对外 RPC 域名');
      return;
    }
    const portNum = parseInt(rpcPort, 10);
    if (Number.isNaN(portNum) || portNum < 1024 || portNum > 65535) {
      setError('RPC 端口需在 1024-65535');
      return;
    }
    setStep('testing');
    try {
      const r = await offchainApi.testClearingBankEndpointConnectivity(rpcDomain.trim(), portNum, peerId);
      setReport(r);
      setStep('tested');
    } catch (e) {
      setError(sanitizeError(e));
      setStep('form');
    }
  }, [peerId, rpcDomain, rpcPort]);

  const startSign = useCallback(async () => {
    if (!selectedSigner) {
      setError('请先在岗位任职页授权具有节点声明权限的岗位');
      return;
    }
    if (!report || !report.allOk) {
      setError('连通性自测未全部通过,无法提交');
      return;
    }
    const portNum = parseInt(rpcPort, 10);
    setError(null);
    setStep('qr');
    setCountdown(90);
    try {
      const r = await offchainApi.buildRegisterClearingBankRequest(
        selectedSigner.accountId,
        cidNumber,
        selectedSigner.roleCode,
        peerId,
        rpcDomain.trim(),
        portNum,
      );
      setSignRequest(r);
    } catch (e) {
      setError(sanitizeError(e));
      setStep('error');
    }
  }, [selectedSigner, report, cidNumber, peerId, rpcDomain, rpcPort]);

  const handleScan = useCallback(async (responseJson: string) => {
    const sr = signRequestRef.current;
    if (!sr || !selectedSigner) return;
    setStep('submit');
    try {
      const r = await offchainApi.submitRegisterClearingBank(
        sr.requestId,
        selectedSigner.accountId,
        sr.expectedPayloadHash,
        cidNumber,
        selectedSigner.roleCode,
        peerId,
        rpcDomain.trim(),
        parseInt(rpcPort, 10),
        sr.signNonce,
        sr.signBlockNumber,
        responseJson,
      );
      setTxHash(r.txHash);
      setStep('done');
      setTimeout(onSuccess, 1500);
    } catch (e) {
      setError(sanitizeError(e));
      setStep('error');
    }
  }, [selectedSigner, cidNumber, peerId, rpcDomain, rpcPort, onSuccess]);

  return (
    <>
      <button className="back-button" onClick={onBack}>← 返回</button>
      <div className="institution-list-header">
        <h2>声明清算行节点</h2>
        <span className="institution-list-summary">{cidFullName} (<IdentityCidText value={cidNumber} />)</span>
      </div>

      {step === 'form' || step === 'testing' || step === 'tested' || step === 'error' ? (
        <>
          <div className="form-group">
            <label>本机 PeerId(自动获取,不可改)</label>
            <code className="institution-card-address">{peerId || '加载中…'}</code>
          </div>
          <div className="form-group">
            <label>对外 RPC 域名</label>
            <input
              type="text"
              placeholder="例如 l2.example.com"
              value={rpcDomain}
              onChange={(e) => setRpcDomain(e.target.value)}
            />
          </div>
          <div className="form-group">
            <label>RPC 端口</label>
            <input
              type="number"
              min={1024}
              max={65535}
              value={rpcPort}
              onChange={(e) => setRpcPort(e.target.value)}
            />
          </div>
          <div className="form-group">
            <label>已授权岗位</label>
            {signers.length === 0 ? (
              <p className="muted">本机构尚无可声明节点的有效激活记录，请先在岗位任职页完成激活。</p>
            ) : signers.length === 1 ? (
              <code className="institution-card-address">{accountIdToSs58(signers[0].accountId)}</code>
            ) : (
              <select
                value={selectedSigner ? `${selectedSigner.personCid}:${selectedSigner.roleCode}` : ''}
                onChange={(e) => {
                  const found = signers.find(a => `${a.personCid}:${a.roleCode}` === e.target.value);
                  setSelectedSigner(found ?? null);
                }}
              >
                <option value="">— 选择岗位 —</option>
                {signers.map((a) => (
                  <option key={`${a.personCid}:${a.roleCode}`} value={`${a.personCid}:${a.roleCode}`}>
                    {a.roleName} · {accountIdToSs58(a.accountId)}
                  </option>
                ))}
              </select>
            )}
          </div>

          <div className="form-actions">
            <button className="secondary-button" onClick={runConnectivityTest} disabled={step === 'testing'}>
              {step === 'testing' ? '自测中…' : '运行连通性自测'}
            </button>
            <button
              className="primary-button"
              onClick={startSign}
              disabled={!report || !report.allOk || !selectedSigner}
            >
              扫码签名并提交
            </button>
          </div>

          {report && (
            <div className="connectivity-report">
              <h3>自测结果 {report.allOk ? '✅' : '❌'}</h3>
              <ul>
                {report.checks.map((c, i) => (
                  <li key={i} className={c.ok ? 'ok' : 'fail'}>
                    <strong>{c.label}</strong>:{c.ok ? '通过' : (c.detail ?? '失败')}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {error && <div className="error">{error}</div>}
        </>
      ) : null}

      <CitizenSignatureModal
        open={step === 'qr' && signRequest != null}
        title="声明清算行节点"
        qrValue={signRequest?.requestJson ?? ''}
        countdownSeconds={countdown}
        onScan={handleScan}
        onScanError={(e) => { setError(e); setStep('error'); }}
        onCancel={() => setStep('tested')}
      />

      {step === 'submit' && (
        <div className="modal-overlay">
          <div className="modal-content">
            <p>正在提交链上交易…</p>
          </div>
        </div>
      )}

      {step === 'done' && txHash && (
        <div className="modal-overlay">
          <div className="modal-content">
            <p>声明成功!</p>
            <code className="institution-card-address">{txHash}</code>
          </div>
        </div>
      )}
    </>
  );
}
