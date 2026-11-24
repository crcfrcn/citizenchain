// 始终展示本机节点配置状态，按有效岗位授权和设备密码管理密钥导入。

import { sanitizeError } from "../client/tauri";
import { settingsApi as api } from "./api";
import type { BootnodeKey } from "./types";
import { useEffect, useRef, useState } from 'react';

type Props = {
  canManage: boolean;
  nodeKey: BootnodeKey;
  onUpdated: (next: BootnodeKey) => void;
  onApplied: () => void;
};

type KeyAction = 'bootnode' | 'grandpa' | 'onchina';
type HostKeyStatus = { configured: boolean | null; error?: string };
// 仅保留公开的配置状态，切换 tab 时先显示上次结果；不缓存任何密钥材料。
let lastHostKeyConfigured: boolean | null = null;
const HOST_KEY_PLACEHOLDER = '请输入链上中国主机HTTPS密钥';

export function NodeKeySection({ canManage, nodeKey, onUpdated, onApplied }: Props) {
  const [input, setInput] = useState('');
  const [grandpaInput, setGrandpaInput] = useState('');
  const [hostKeyInput, setHostKeyInput] = useState('');
  const [grandpaConfigured, setGrandpaConfigured] = useState<boolean | null>(null);
  const [hostKeyStatus, setHostKeyStatus] = useState<HostKeyStatus>(() => ({ configured: lastHostKeyConfigured }));
  const hostKeyUploaded = useRef(false);
  const [unlockPassword, setUnlockPassword] = useState('');
  const [pendingAction, setPendingAction] = useState<KeyAction | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 配置状态始终可见；授权失效时立即清除输入与待确认密码，不保留可提交草稿。
  useEffect(() => {
    if (canManage) return;
    setInput('');setGrandpaInput('');setHostKeyInput('');
    setUnlockPassword('');setPendingAction(null);setError(null);
  }, [canManage]);

  useEffect(() => {
    // 保持原有授权后的读取边界，未核验的配置不能显示为“未设置”。
    if (!canManage) return;
    hostKeyUploaded.current = false;
    let cancelled = false;
    void api.getGrandpaKey().then((key) => {
      if (!cancelled) setGrandpaConfigured(Boolean(key.cidShortName));
    }).catch(() => undefined);
    void api.getOnChinaHostKeyStatus().then((configured) => {
      if (!cancelled && !hostKeyUploaded.current) {
        lastHostKeyConfigured = configured;
        setHostKeyStatus({ configured });
      }
    }).catch((error) => {
      if (!cancelled && !hostKeyUploaded.current) {
        setHostKeyStatus((current) => ({ ...current, error: sanitizeError(error) }));
      }
    });
    return () => { cancelled = true; };
  }, [canManage]);

  function requestUpload(action: KeyAction, value: string, emptyMessage: string) {
    if (!canManage) return;
    if (!value.trim()) {
      setError(emptyMessage);
      return;
    }
    setError(null);
    setUnlockPassword('');
    setPendingAction(action);
  }

  function closePasswordModal() {
    if (saving) return;
    setPendingAction(null);
    setUnlockPassword('');
    setError(null);
  }

  async function uploadKey() {
    if (!canManage || pendingAction === null) return;
    const password = unlockPassword.trim();
    if (!password) {
      setError('请输入设备开机密码');
      return;
    }
    setSaving(true);
    setError(null);
    try {
      switch (pendingAction) {
        case 'bootnode': {
          const next = await api.setBootnodeKey(input.trim(), password);
          onUpdated(next);
          onApplied();
          setInput('');
          break;
        }
        case 'grandpa': {
          const next = await api.setGrandpaKey(grandpaInput.trim(), password);
          setGrandpaConfigured(Boolean(next.cidShortName));
          setGrandpaInput('');
          break;
        }
        case 'onchina': {
          await api.setOnChinaHostKey(hostKeyInput.trim(), password);
          hostKeyUploaded.current = true;
          lastHostKeyConfigured = true;
          setHostKeyStatus({ configured: true });
          setHostKeyInput('');
          break;
        }
      }
      setPendingAction(null);
    } catch (error) {
      setError(sanitizeError(error));
    } finally {
      setUnlockPassword('');
      setSaving(false);
    }
  }

  return (
    <section className="section settings-nodekey-section">
      <div className="settings-credential-row">
        <div className="settings-credential-label">
          <h2>引导节点</h2>
          <span className="settings-credential-state">{nodeKey.peerId ? '已设置' : '未设置'}</span>
        </div>
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="请输入引导节点私钥（Ed25519 十六进制）"
          type="password"
          disabled={saving || !canManage}
        />
        <button
          className="settings-credential-action"
          disabled={saving || !canManage}
          onClick={() => requestUpload('bootnode', input, '请输入引导节点私钥')}
        >
          {saving && pendingAction === 'bootnode' ? '绑定中...' : '绑定身份'}
        </button>
      </div>
      <div className="settings-credential-row">
        <div className="settings-credential-label">
          <h2>最终性节点</h2>
          <span className="settings-credential-state">{grandpaConfigured === null ? '未核验' : grandpaConfigured ? '已设置' : '未设置'}</span>
        </div>
        <input
          value={grandpaInput}
          onChange={(e) => setGrandpaInput(e.target.value)}
          placeholder="请输入最终性节点私钥"
          type="password"
          disabled={saving || !canManage}
        />
        <button
          className="settings-credential-action"
          disabled={saving || !canManage}
          onClick={() => requestUpload('grandpa', grandpaInput, '请输入最终性节点私钥')}
        >
          {saving && pendingAction === 'grandpa' ? '上传中...' : '上传私钥'}
        </button>
      </div>
      <div className="settings-credential-row">
        <div className="settings-credential-label">
          <h2>链上中国主机</h2>
          <span className="settings-credential-state">
            {hostKeyStatus.configured === null ? '未核验' : hostKeyStatus.configured ? '已设置' : '未设置'}
          </span>
        </div>
        <input
          value={hostKeyInput}
          onChange={(e) => setHostKeyInput(e.target.value)}
          onPaste={(e) => {
            const pasted = e.clipboardData.getData('text');
            if (/[\r\n]/.test(pasted)) {
              // 单行密码框会丢弃换行，直接保留剪贴板中的多行 PEM。
              e.preventDefault();
              setHostKeyInput(pasted);
            }
          }}
          placeholder={HOST_KEY_PLACEHOLDER}
          type="password"
          autoComplete="off"
          spellCheck={false}
          disabled={saving || !canManage}
        />
        <button
          className="settings-credential-action"
          aria-label="上传链上中国主机私钥"
          disabled={saving || !canManage}
          onClick={() => requestUpload('onchina', hostKeyInput, HOST_KEY_PLACEHOLDER)}
        >
          {saving && pendingAction === 'onchina' ? '上传中...' : '上传私钥'}
        </button>
      </div>
      {hostKeyStatus.error && <p className="section-inline-error">{hostKeyStatus.error}</p>}
      {!pendingAction && error && <p className="section-inline-error">{error}</p>}

      {pendingAction && (
        <div className="unlock-modal-mask" onClick={closePasswordModal}>
          <div className="unlock-modal" onClick={(e) => e.stopPropagation()}>
            <h3>设备密码验证</h3>
            <input
              className="unlock-password-input"
              type="password"
              value={unlockPassword}
              onChange={(e) => setUnlockPassword(e.target.value)}
              placeholder="请输入设备开机密码"
              disabled={saving || !canManage}
            />
            {error && <p className="section-inline-error">{error}</p>}
            <div className="unlock-modal-actions">
              <button onClick={closePasswordModal} disabled={saving || !canManage}>取消</button>
              <button onClick={uploadKey} disabled={saving || !canManage}>
                {saving ? '验证中...' : '确认'}
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
