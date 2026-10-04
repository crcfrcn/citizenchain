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
import { sanitizeError } from '../../tauri';
import { adminsChangeApi } from '../../admins/api';
import { accountIdToSs58 } from '../../shared/ss58';
import { CitizenSignatureModal } from '../../shared/qr/CitizenSignatureModal';
import type { ActivatedAdmin, AdminSignerMatch, VoteSignRequestResult } from '../../governance/types';
import { offchainApi } from './api';
import type { ConnectivityTestReport } from './types';

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

  // 管理员 = 已激活的清算行管理员中第一个;Step 3 完工后改为下拉选(支持多管理员)
  const [admins, setAdmins] = useState<AdminSignerMatch[]>([]);
  const [selectedAdmin, setSelectedAdmin] = useState<AdminSignerMatch | null>(null);

  // QR 流程
  const [signRequest, setSignRequest] = useState<VoteSignRequestResult | null>(null);
  const [countdown, setCountdown] = useState(90);
  const [txHash, setTxHash] = useState<string | null>(null);

  const signRequestRef = useRef(signRequest);
  signRequestRef.current = signRequest;

  // 初始化：拉本机 PeerId + 按机构 CID 拉已激活管理员。
  useEffect(() => {
    let cancelled = false;
    Promise.all([
      offchainApi.queryLocalPeerId().catch(() => ''),
      adminsChangeApi.getActivatedAdmins(cidNumber, { cidNumber })
        .catch(() => [] as ActivatedAdmin[]),
    ]).then(([pid, aa]) => {
      if (cancelled) return;
      setPeerId(pid);
      const matches: AdminSignerMatch[] = aa.map(a => ({
        ss58_address: accountIdToSs58(a.account_id),
        account_id: a.account_id,
        account_label: '',
      }));
      setAdmins(matches);
      if (matches.length === 1) setSelectedAdmin(matches[0]);
    });
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
    if (!selectedAdmin) {
      setError('请先在治理-管理员列表激活管理员后再来声明节点');
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
        selectedAdmin.account_id,
        cidNumber,
        peerId,
        rpcDomain.trim(),
        portNum,
      );
      setSignRequest(r);
    } catch (e) {
      setError(sanitizeError(e));
      setStep('error');
    }
  }, [selectedAdmin, report, cidNumber, peerId, rpcDomain, rpcPort]);

  const handleScan = useCallback(async (responseJson: string) => {
    const sr = signRequestRef.current;
    if (!sr || !selectedAdmin) return;
    setStep('submit');
    try {
      const r = await offchainApi.submitRegisterClearingBank(
        sr.requestId,
        selectedAdmin.account_id,
        sr.expectedPayloadHash,
        cidNumber,
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
  }, [selectedAdmin, cidNumber, peerId, rpcDomain, rpcPort, onSuccess]);

  return (
    <>
      <button className="back-button" onClick={onBack}>← 返回</button>
      <div className="admin-list-header">
        <h2>声明清算行节点</h2>
        <span className="admin-list-summary">{cidFullName} ({cidNumber})</span>
      </div>

      {step === 'form' || step === 'testing' || step === 'tested' || step === 'error' ? (
        <>
          <div className="form-group">
            <label>本机 PeerId(自动获取,不可改)</label>
            <code className="admin-card-address">{peerId || '加载中…'}</code>
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
            <label>已激活管理员</label>
            {admins.length === 0 ? (
              <p className="muted">本机构尚无已激活管理员。请先回到治理 tab 激活至少 1 名清算行管理员后再来声明节点。</p>
            ) : admins.length === 1 ? (
              <code className="admin-card-address">{admins[0].ss58_address}</code>
            ) : (
              <select
                value={selectedAdmin?.account_id ?? ''}
                onChange={(e) => {
                  const found = admins.find(a => a.account_id === e.target.value);
                  setSelectedAdmin(found ?? null);
                }}
              >
                <option value="">— 选择管理员 —</option>
                {admins.map((a) => (
                  <option key={a.account_id} value={a.account_id}>
                    {a.ss58_address}
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
              disabled={!report || !report.allOk || !selectedAdmin}
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
            <code className="admin-card-address">{txHash}</code>
          </div>
        </div>
      )}
    </>
  );
}
