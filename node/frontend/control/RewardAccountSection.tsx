import { sanitizeError } from "../client/tauri";
import { AddressScanModal } from "../desktop/scanner";
import { accountIdToSs58, ADDRESS_INPUT_PLACEHOLDER, normalizeSs58Address } from "../client/ss58";
import { settingsApi as api } from "./api";
import type { RewardAccount } from "./types";
import scanIcon from "../../../icons/scan.svg?url";
import { useState, useEffect, useCallback } from 'react';

type Props = {
  rewardAccount: RewardAccount;
  onUpdated: (next: RewardAccount) => void;
};

type BindStatus = null | 'binding' | 'success' | 'failed' | 'timeout';

/** 管理奖励账户；账户 ID 是唯一标识，SS58 地址仅用于输入与展示。 */
export function RewardAccountSection({ rewardAccount, onUpdated }: Props) {
  const [input, setInput] = useState('');
  const [showPasswordModal, setShowPasswordModal] = useState(false);
  const [unlockPassword, setUnlockPassword] = useState('');
  const [pendingAddress, setPendingAddress] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [bindStatus, setBindStatus] = useState<BindStatus>(null);
  const [showAddressScan, setShowAddressScan] = useState(false);
  const boundAddress = rewardAccount.accountId ? accountIdToSs58(rewardAccount.accountId) : null;
  const hasBoundAddress = boundAddress !== null;
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
      setInput('');
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
      <div className="settings-credential-row">
        <h2>矿工收款账户</h2>
        <span className="settings-credential-state">{hasBoundAddress ? '已绑定' : '未绑定'}</span>
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder={ADDRESS_INPUT_PLACEHOLDER}
          disabled={saving}
        />
        <button type="button" className="scan-icon-btn" onClick={() => setShowAddressScan(true)} disabled={saving} title="扫码填入">
          <span aria-hidden="true" style={{display:"inline-block",width:18,height:18,backgroundColor:"currentColor",maskImage:`url("${scanIcon}")`,WebkitMaskImage:`url("${scanIcon}")`,maskSize:"contain",WebkitMaskSize:"contain",maskRepeat:"no-repeat",WebkitMaskRepeat:"no-repeat"}} />
        </button>
        {boundAddress && (
          <span className="settings-credential-address" title={boundAddress}>
            {boundAddress.length > 16 ? `${boundAddress.slice(0, 6)}…${boundAddress.slice(-10)}` : boundAddress}
          </span>
        )}
        <button
          className="settings-credential-action"
          disabled={saving}
          onClick={() => {
            let nextAddress = '';
            try {
              const address = input.trim();
              nextAddress = address.startsWith('0x')
                ? address
                : normalizeSs58Address(address, ADDRESS_INPUT_PLACEHOLDER);
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
          onResult={({ ss58Address }) => { setInput(ss58Address); setShowAddressScan(false); }}
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
