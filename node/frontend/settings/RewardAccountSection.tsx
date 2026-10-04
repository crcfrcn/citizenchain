import { useState, useEffect, useCallback } from 'react';
import { sanitizeError } from '../tauri';
import { AddressScanModal } from '../shared/qr/AddressScanModal';
import { normalizeSs58Address } from '../shared/ss58';
import { settingsApi as api } from './api';
import type { RewardAccount } from './types';

type Props = {
  rewardAccount: RewardAccount;
  onUpdated: (next: RewardAccount) => void;
};

type BindStatus = null | 'binding' | 'success' | 'failed' | 'timeout';

/** 管理奖励账户；账户 ID 是唯一标识，SS58 地址仅用于输入与展示。 */
export function RewardAccountSection({ rewardAccount, onUpdated }: Props) {
  const [input, setInput] = useState(rewardAccount.ss58_address ?? '');
  const [showPasswordModal, setShowPasswordModal] = useState(false);
  const [unlockPassword, setUnlockPassword] = useState('');
  const [pendingAddress, setPendingAddress] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [bindStatus, setBindStatus] = useState<BindStatus>(null);
  const [showAddressScan, setShowAddressScan] = useState(false);
  const [minerAddress, setMinerAddress] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api.getLocalMinerSs58Address()
      .then((addr) => {
        if (!cancelled) setMinerAddress(addr);
      })
      .catch(() => {
        if (!cancelled) setMinerAddress(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);
  const hasBoundAddress = Boolean(rewardAccount.account_id);
  const actionText = hasBoundAddress ? '变更地址' : '绑定地址';

  // 监听后台链上绑定结果事件（仅在用户主动发起绑定后才响应）
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        unlisten = await listen<{ status: string; detail: string }>(
          'reward-account-bind-result',
          (event) => {
            if (cancelled) return;
            setBindStatus((prev) => {
              if (prev !== 'binding') return prev;
              const { status } = event.payload;
              if (status === 'success') {
                return 'success';
              } else if (status === 'timeout') {
                setError('地址已保存，但链上绑定超时，将在下次启动时重试');
                return 'timeout';
              } else {
                setError(`地址已保存，但链上绑定失败：${event.payload.detail}`);
                return 'failed';
              }
            });
          },
        );
      } catch {
        // listen 不可用时静默降级
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const onSubmit = useCallback(async () => {
    const password = unlockPassword.trim();
    if (!password) {
      setError('请输入设备开机密码');
      return;
    }
    if (!pendingAddress) {
      setError('地址为空，请重新输入');
      return;
    }
    setSaving(true);
    try {
      const next = await api.setRewardAccount(pendingAddress, password);
      onUpdated(next);
      setInput(next.ss58_address ?? '');
      setShowPasswordModal(false);
      setPendingAddress(null);
      setError(null);
      setBindStatus('binding');
    } catch (e) {
      setError(sanitizeError(e));
    } finally {
      setUnlockPassword('');
      setSaving(false);
    }
  }, [unlockPassword, pendingAddress, onUpdated]);

  const bindHint = bindStatus === 'binding'
    ? '链上绑定中，请稍候...'
    : bindStatus === 'success'
      ? '已绑定'
      : null;

  return (
    <section className="section settings-reward-account-section">
      <div className="reward-account-inline reward-account-inline-readonly">
        <span className="reward-account-current">
          本机矿工账户
          <span className="reward-account-bind-state">{minerAddress ?? '未生成（节点启动中…）'}</span>
        </span>
      </div>
      <div className="reward-account-inline">
        <span className="reward-account-current">
          手续费收款地址
          <span className="reward-account-bind-state">{rewardAccount.ss58_address ?? '未绑定'}</span>
        </span>
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="请输入手续费收款账户的 SS58 地址"
          disabled={saving}
        />
        <button type="button" className="scan-icon-btn" onClick={() => setShowAddressScan(true)} disabled={saving} title="扫码填入">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M3 7V5a2 2 0 0 1 2-2h2"/><path d="M17 3h2a2 2 0 0 1 2 2v2"/><path d="M21 17v2a2 2 0 0 1-2 2h-2"/><path d="M7 21H5a2 2 0 0 1-2-2v-2"/>
            <rect x="7" y="7" width="10" height="10" rx="1"/>
          </svg>
        </button>
        <button
          disabled={saving}
          onClick={() => {
            let nextAddress = '';
            try {
              nextAddress = normalizeSs58Address(input, '请输入手续费收款账户的 SS58 地址');
            } catch (e) {
              setError(sanitizeError(e));
              return;
            }
            setError(null);
            setPendingAddress(nextAddress);
            setUnlockPassword('');
            setShowPasswordModal(true);
          }}
        >
          {saving ? '保存中...' : actionText}
        </button>
      </div>
      {bindHint ? <p className="section-inline-hint">{bindHint}</p> : null}
      {error ? <p className="section-inline-error">{error}</p> : null}

      {showAddressScan && (
        <AddressScanModal
          onResult={({ ss58_address }) => { setInput(ss58_address); setShowAddressScan(false); }}
          onClose={() => setShowAddressScan(false)}
        />
      )}

      {showPasswordModal ? (
        <div className="unlock-modal-mask" onClick={() => !saving && setShowPasswordModal(false)}>
          <div className="unlock-modal" onClick={(e) => e.stopPropagation()}>
            <h3>设备密码验证</h3>
            <input
              className="unlock-password-input"
              type="password"
              value={unlockPassword}
              onChange={(e) => setUnlockPassword(e.target.value)}
              placeholder="请输入设备开机密码"
              disabled={saving}
            />
            <div className="unlock-modal-actions">
              <button
                onClick={() => setShowPasswordModal(false)}
                disabled={saving}
              >
                取消
              </button>
              <button
                onClick={onSubmit}
                disabled={saving}
              >
                {saving ? '验证中...' : actionText}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}
