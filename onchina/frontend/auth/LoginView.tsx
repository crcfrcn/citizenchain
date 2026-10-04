// 独立的登录页组件:渲染未登录态(QR 扫码登录 + 签名请求/响应),
// 持有登录相关 state / handler / useEffect / videoRef,
// 登录成功后通过 useAuth().setAuth 写入全局,App.tsx 只负责在 !auth 时渲染 <LoginView />。

import { useCallback, useEffect, useState } from 'react';
import { Alert, Button, Modal, Radio, Space, Typography } from 'antd';
import { DownloadOutlined, QrcodeOutlined } from '@ant-design/icons';
import { useAuth } from '../hooks/useAuth';
import { writeStoredAuth } from '../utils/storedAuth';
import { parseSignedLoginPayload } from '../utils/parseSignedPayload';
import { CitizenSignaturePanel } from '../core/CitizenSignaturePanel';
import { parseQrEnvelope } from '../core/citizenQr';
import type { AccountIdCodeBody } from '../core/citizenQr';
import type { AdminAuth } from './types';
import type { AdminIdentifyResult, AdminQrSignRequestResult, NodeBindingRequired } from './api';
import {
  completeAdminQrLogin,
  confirmNodeBinding,
  createAdminQrSignRequest,
  queryAdminQrLoginResult,
} from './api';
import { notice } from '../utils/notice';

function createSessionId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `sid-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

const CA_CERTIFICATE_URL = '/api/platform/ca-certificate';

/// 账户短址（首 6 后 6），只用于让操作员核对扫到的是哪个账户。
function shortAccountId(account_id: string): string {
  return account_id.length > 16
    ? `${account_id.slice(0, 8)}…${account_id.slice(-6)}`
    : account_id;
}

function shouldShowOrganizationCaNotice(): boolean {
  if (typeof window === 'undefined') return false;
  // 浏览器不开放读取系统根证书库;前端只能用当前页面是否为可信 HTTPS
  // 安全上下文判断证书安装结果。证书被信任后登录页和后台页都不再显示 CA 提示。
  return !(window.location.protocol === 'https:' && window.isSecureContext);
}

export function OrganizationCaNotice({ compact = false }: { compact?: boolean }) {
  if (!shouldShowOrganizationCaNotice()) return null;
  const isHttps = typeof window !== 'undefined' && window.location.protocol === 'https:';
  return (
    <Alert
      type="warning"
      showIcon
      style={{ marginBottom: compact ? 16 : 24, borderRadius: 8 }}
      message={isHttps ? '当前浏览器尚未信任本节点证书' : '当前页面不是 HTTPS 安全环境'}
      description={
        <Space direction={compact ? 'horizontal' : 'vertical'} size={8} wrap style={{ width: '100%' }}>
          <Typography.Text>
            初次使用请先下载并安装本机构节点 CA 证书，安装完成后关闭并重新打开浏览器，再使用摄像头扫码和 passkey。
          </Typography.Text>
          {!compact ? (
            <Typography.Text type="secondary">
              Windows 选择“受信任的根证书颁发机构”；macOS 在“钥匙串访问”中将证书设为“始终信任”。如出现 -25294，先删除同名旧证书后重新导入。
            </Typography.Text>
          ) : null}
          <Button
            type="primary"
            size="small"
            icon={<DownloadOutlined />}
            href={CA_CERTIFICATE_URL}
            style={{ width: 'fit-content' }}
          >
            下载机构 CA 证书
          </Button>
        </Space>
      }
    />
  );
}

export function LoginView() {
  const { auth, setAuth } = useAuth();
  const [pendingQrLogin, setPendingQrLogin] = useState<AdminQrSignRequestResult | null>(null);
  const [pendingBinding, setPendingBinding] = useState<NodeBindingRequired | null>(null);
  const [displayName, setDisplayName] = useState('');
  const [selectedCandidateId, setSelectedCandidateId] = useState('');
  const [challengeLoading, setChallengeLoading] = useState(false);
  const [scanSubmitting, setScanSubmitting] = useState(false);
  const [bindingSubmitting, setBindingSubmitting] = useState(false);

  const finishLogin = useCallback((accessToken: string, admin: AdminIdentifyResult) => {
    const nextAuth: AdminAuth = {
      access_token: accessToken,
      account_id: admin.account_id,
      institution_cid_number: admin.institution_cid_number,
      institution_code: admin.institution_code,
      admin_level: admin.admin_level ?? null,
      capabilities: admin.capabilities,
      workspace: admin.workspace,
      family_name: admin.family_name,
      given_name: admin.given_name,
      scope_province_name: admin.scope_province_name ?? null,
      scope_city_name: admin.scope_city_name ?? null,
      scope_town_name: admin.scope_town_name ?? null,
      cid_short_name: admin.cid_short_name ?? null,
    };
    setAuth(nextAuth);
    writeStoredAuth(nextAuth);
    setPendingQrLogin(null);
    setPendingBinding(null);
    setSelectedCandidateId('');
    setDisplayName('');
    notice.success('登录成功');
  }, [setAuth]);

  // 登录第 1 步只收账户码：管理员私钥保管在离线的 CitizenWallet，账户码由它自己就能
  // 出示，不需要联网热钱包参与。CID 与管理员姓名一律由链上反查，不信二维码自述。
  const onScanIdentity = useCallback(async (raw: string) => {
    setChallengeLoading(true);
    try {
      const walletCode = parseQrEnvelope(raw);
      if (walletCode.kind !== 'account_id_code') {
        throw new Error(
          `必须扫描 QR_V1 账户码（钱包 → 账户详情右上角二维码），当前类型为 ${walletCode.kind}`,
        );
      }
      const body = walletCode.body as AccountIdCodeBody;
      const sessionId = createSessionId();
      const origin = window.location.origin;
      const challenge = await createAdminQrSignRequest({
        identity_qr: raw,
        origin,
        session_id: sessionId,
      });
      // 账户码不带显示名，这里展示账户短址供操作员核对扫到的是哪个账户；
      // 管理员姓名在会话签发后由链上/数据库记录提供，不信二维码自述。
      setDisplayName(shortAccountId(body.account_id));
      setPendingQrLogin(challenge);
      notice.success('已生成该管理员专用登录二维码');
    } catch (err) {
      notice.error(err, '账户码读取失败');
      setDisplayName('');
      setPendingQrLogin(null);
    } finally {
      setChallengeLoading(false);
    }
  }, []);

  const resetIdentityScan = () => {
    setPendingQrLogin(null);
    setDisplayName('');
  };

  const onCompleteSignedLogin = useCallback(async (raw: string) => {
    if (!pendingQrLogin) {
      notice.error('请先重新扫描管理员账户码');
      return;
    }
    setScanSubmitting(true);
    try {
      const payload = parseSignedLoginPayload(raw, pendingQrLogin.challenge_id);
      const completion = await completeAdminQrLogin({
        challenge_id: payload.challenge_id,
        session_id: pendingQrLogin.session_id,
        account_id: payload.account_id,
        signature: payload.signature,
      });
      if (completion.status === 'BINDING_REQUIRED' && completion.binding) {
        setPendingBinding(completion.binding);
        setSelectedCandidateId(completion.binding.candidates[0]?.candidate_id ?? '');
        notice.success('请选择本节点绑定机构');
        return;
      }
      if (completion.status === 'SUCCESS' && completion.access_token && completion.admin) {
        finishLogin(completion.access_token, completion.admin);
        return;
      }
      const status = await queryAdminQrLoginResult(
        pendingQrLogin.challenge_id,
        pendingQrLogin.session_id,
      );
      if (status.status === 'SUCCESS' && status.access_token && status.admin) {
        finishLogin(status.access_token, status.admin);
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : '';
      if (msg.toLowerCase().includes('admin not found')) {
        notice.error('非管理员禁止登录本系统');
      } else {
        notice.error(err, '登录签名响应处理失败');
      }
    } finally {
      setScanSubmitting(false);
    }
  }, [finishLogin, pendingQrLogin]);

  const onConfirmBinding = async () => {
    if (!pendingBinding || !selectedCandidateId) return;
    setBindingSubmitting(true);
    try {
      const result = await confirmNodeBinding({
        binding_challenge_id: pendingBinding.binding_challenge_id,
        candidate_id: selectedCandidateId,
      });
      finishLogin(result.access_token, result.admin);
    } catch (err) {
      notice.error(err, '绑定机构失败');
    } finally {
      setBindingSubmitting(false);
    }
  };

  // QR 后台轮询 effect
  useEffect(() => {
    if (auth || !pendingQrLogin) return;
    let cancelled = false;
    const timer = window.setInterval(async () => {
      try {
        const status = await queryAdminQrLoginResult(
          pendingQrLogin.challenge_id,
          pendingQrLogin.session_id,
        );
        if (cancelled) return;
        if (status.status === 'PENDING') return;
        if (status.status === 'EXPIRED') {
          notice.warning('二维码已过期，请重新扫描管理员账户码');
          setPendingQrLogin(null);
          setDisplayName('');
          return;
        }
        if (status.status === 'SUCCESS' && status.access_token && status.admin) {
          finishLogin(status.access_token, status.admin);
        }
      } catch {
        // keep polling
      }
    }, 1200);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [auth, finishLogin, pendingQrLogin]);

  return (
    <div
      style={{
        width: 780,
        maxWidth: '95vw',
        background: 'rgba(255,255,255,0.92)',
        backdropFilter: 'blur(20px)',
        borderRadius: 20,
        boxShadow: '0 8px 40px rgba(0,0,0,0.12), 0 1px 3px rgba(0,0,0,0.06)',
        border: '1px solid rgba(255,255,255,0.6)',
        overflow: 'hidden',
      }}
    >
      {/* 登录卡片顶部 */}
      <div
        style={{
          background: 'linear-gradient(135deg, #0d9488 0%, #0f766e 50%, #115e59 100%)',
          padding: '28px 32px',
          textAlign: 'center',
        }}
      >
        <div
          style={{
            display: 'inline-flex',
            alignItems: 'center',
            justifyContent: 'center',
            width: 52,
            height: 52,
            borderRadius: 14,
            background: 'rgba(255,255,255,0.18)',
            marginBottom: 12,
            border: '1px solid rgba(255,255,255,0.25)',
          }}
        >
          <QrcodeOutlined style={{ fontSize: 26, color: '#fff' }} />
        </div>
        <Typography.Title
          level={4}
          style={{ color: '#fff', margin: 0, fontWeight: 600, letterSpacing: 1 }}
        >
          管理员扫码登录
        </Typography.Title>
        <Typography.Text style={{ color: 'rgba(255,255,255,0.7)', fontSize: 13 }}>
          用公民钱包出示账户详情的账户码，再由同一钱包签名登录
        </Typography.Text>
      </div>

      {/* 登录内容区域 */}
      <div style={{ padding: '32px 36px 36px' }}>
        <OrganizationCaNotice />
        <CitizenSignaturePanel
          qrTitle={pendingQrLogin ? '定向登录二维码' : '登录请求'}
          qrValue={pendingQrLogin?.login_qr_payload}
          qrPlaceholderValue="ONCHINA_LOGIN_PENDING"
          qrHint={
            pendingQrLogin
              ? `${displayName} · 有效期至 ${new Date(pendingQrLogin.expire_at * 1000).toLocaleTimeString()}`
              : '扫描管理员账户码后生成，仅该账户可以签名'
          }
          scannerTitle={pendingQrLogin ? '签名响应' : '管理员账户码'}
          scannerHint={
            pendingQrLogin
              ? '扫描公民钱包生成的签名响应二维码'
              : '读取 QR_V1 账户码中的 account_id；CID 与管理员姓名一律由链上反查'
          }
          primaryActionText={pendingQrLogin ? '重新扫描账户码' : undefined}
          primaryActionLoading={challengeLoading}
          onPrimaryAction={resetIdentityScan}
          scannerButtonText={pendingQrLogin ? '扫描签名响应' : '扫描管理员账户码'}
          scannerDisabled={challengeLoading || scanSubmitting}
          scannerLoading={challengeLoading || scanSubmitting}
          onDetected={pendingQrLogin ? onCompleteSignedLogin : onScanIdentity}
          onScannerError={(msg) => notice.error(msg)}
        />
        <Modal
          title="绑定本节点机构"
          open={!!pendingBinding}
          onCancel={() => !bindingSubmitting && setPendingBinding(null)}
          footer={[
            <Button key="cancel" disabled={bindingSubmitting} onClick={() => setPendingBinding(null)}>
              取消
            </Button>,
            <Button
              key="confirm"
              type="primary"
              loading={bindingSubmitting}
              disabled={!selectedCandidateId}
              onClick={onConfirmBinding}
            >
              确认绑定
            </Button>,
          ]}
        >
          <Radio.Group
            value={selectedCandidateId}
            onChange={(event) => setSelectedCandidateId(event.target.value)}
            style={{ width: '100%' }}
          >
            <Space direction="vertical" style={{ width: '100%' }}>
              {pendingBinding?.candidates.map((candidate) => {
                const title = candidate.cid_short_name || candidate.cid_full_name || candidate.institution_code;
                const scope = [candidate.scope_province_name, candidate.scope_city_name, candidate.scope_town_name]
                  .filter(Boolean)
                  .join(' / ');
                return (
                  <Radio key={candidate.candidate_id} value={candidate.candidate_id}>
                    <div>
                      <Typography.Text strong>{title}</Typography.Text>
                      <br />
                      <Typography.Text type="secondary">
                        {candidate.institution_code}
                        {candidate.institution_cid_number ? ` · ${candidate.institution_cid_number}` : ''}
                        {scope ? ` · ${scope}` : ''}
                      </Typography.Text>
                    </div>
                  </Radio>
                );
              })}
            </Space>
          </Radio.Group>
        </Modal>
      </div>
    </div>
  );
}
