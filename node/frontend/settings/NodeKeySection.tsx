import { useEffect, useState } from 'react';
import { sanitizeError } from '../tauri';
import { settingsApi as api } from './api';
import type { BootnodeKey } from './types';

type Props = {
  nodeKey: BootnodeKey;
  onUpdated: (next: BootnodeKey) => void;
  onApplied: () => void;
};

export function NodeKeySection({ nodeKey, onUpdated, onApplied }: Props) {
  type PendingAction = 'bootnode' | 'grandpa' | null;

  const [input, setInput] = useState(nodeKey.nodeKey ?? '');
  const [bootnodeCidShortName, setBootnodeCidShortName] = useState<string | null>(
    nodeKey.cidShortName ?? null,
  );
  const [grandpaInput, setGrandpaInput] = useState('');
  const [grandpaCidShortName, setGrandpaCidShortName] = useState<string | null>(null);
  const [showPasswordModal, setShowPasswordModal] = useState(false);
  const [unlockPassword, setUnlockPassword] = useState('');
  const [pendingAction, setPendingAction] = useState<PendingAction>(null);
  const [saving, setSaving] = useState(false);
  const [savingGrandpa, setSavingGrandpa] = useState(false);
  const [bootnodeError, setBootnodeError] = useState<string | null>(null);
  const [grandpaError, setGrandpaError] = useState<string | null>(null);

  useEffect(() => {
    setInput(nodeKey.nodeKey ?? '');
    setBootnodeCidShortName(nodeKey.cidShortName ?? null);
  }, [nodeKey.nodeKey, nodeKey.cidShortName]);

  useEffect(() => {
    let cancelled = false;
    void api
      .getGrandpaKey()
      .then((k) => {
        if (cancelled) return;
        setGrandpaInput(k.key ?? '');
        setGrandpaCidShortName(k.cidShortName ?? null);
      })
      .catch(() => undefined);

    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <section className="section settings-nodekey-section">
      <div className="bootnode-inline">
        <h2>
          节点身份密钥
          <span className="grandpa-bind-state">
            {bootnodeCidShortName ?? '未绑定'}
          </span>
        </h2>
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="请输入节点身份密钥（Ed25519 私钥 hex）"
          type="password"
          disabled={saving}
        />
        <button
          disabled={saving}
          onClick={() => {
            if (!input.trim()) {
              setBootnodeError('请输入节点身份密钥');
              return;
            }
            setBootnodeError(null);
            setGrandpaError(null);
            setPendingAction('bootnode');
            setUnlockPassword('');
            setShowPasswordModal(true);
          }}
        >
          {saving ? '绑定中...' : '绑定身份'}
        </button>
      </div>
      <div className="bootnode-inline grandpa-inline">
        <h2>
          确定性投票节点
          <span className="grandpa-bind-state">
            {grandpaCidShortName ?? '未绑定'}
          </span>
        </h2>
        <input
          value={grandpaInput}
          onChange={(e) => setGrandpaInput(e.target.value)}
          placeholder="请输入确定性投票节点私钥"
          type="password"
          disabled={savingGrandpa}
        />
        <button
          disabled={savingGrandpa}
          onClick={() => {
            if (!grandpaInput.trim()) {
              setGrandpaError('请输入确定性投票节点私钥');
              return;
            }
            setGrandpaError(null);
            setBootnodeError(null);
            setPendingAction('grandpa');
            setUnlockPassword('');
            setShowPasswordModal(true);
          }}
        >
          {savingGrandpa ? '上传中...' : '上传私钥'}
        </button>
      </div>
      {bootnodeError ? <p className="section-inline-error">{bootnodeError}</p> : null}
      {grandpaError ? <p className="section-inline-error">{grandpaError}</p> : null}

      {showPasswordModal ? (
        <div
          className="unlock-modal-mask"
          onClick={() => {
            if (!(saving || savingGrandpa)) {
              setShowPasswordModal(false);
              setUnlockPassword('');
              setPendingAction(null);
            }
          }}
        >
          <div className="unlock-modal" onClick={(e) => e.stopPropagation()}>
            <h3>设备密码验证</h3>
            <input
              className="unlock-password-input"
              type="password"
              value={unlockPassword}
              onChange={(e) => setUnlockPassword(e.target.value)}
              placeholder="请输入设备开机密码"
              disabled={saving || savingGrandpa}
            />
            <div className="unlock-modal-actions">
              <button
                onClick={() => {
                  setShowPasswordModal(false);
                  setUnlockPassword('');
                  setPendingAction(null);
                }}
                disabled={saving || savingGrandpa}
              >
                取消
              </button>
              <button
                onClick={async () => {
                  const password = unlockPassword.trim();
                  if (!password) {
                    if (pendingAction === 'grandpa') {
                      setGrandpaError('请输入设备开机密码');
                    } else {
                      setBootnodeError('请输入设备开机密码');
                    }
                    return;
                  }

                  if (pendingAction === 'bootnode') {
                    setSaving(true);
                    try {
                      const next = await api.setBootnodeKey(input.trim(), password);
                      onUpdated(next);
                      onApplied();
                      setBootnodeCidShortName(next.cidShortName ?? null);
                      setInput('');
                      setShowPasswordModal(false);
                      setPendingAction(null);
                      setBootnodeError(null);
                    } catch (e) {
                      setBootnodeError(sanitizeError(e));
                    } finally {
                      setUnlockPassword('');
                      setSaving(false);
                    }
                    return;
                  }

                  if (pendingAction === 'grandpa') {
                    setSavingGrandpa(true);
                    try {
                      const next = await api.setGrandpaKey(grandpaInput.trim(), password);
                      setGrandpaInput('');
                      setGrandpaCidShortName(next.cidShortName ?? null);
                      setShowPasswordModal(false);
                      setPendingAction(null);
                      setGrandpaError(null);
                    } catch (e) {
                      setGrandpaError(sanitizeError(e));
                    } finally {
                      setUnlockPassword('');
                      setSavingGrandpa(false);
                    }
                    return;
                  }

                  setBootnodeError('未选择上传类型，请重新点击上传按钮');
                }}
                disabled={saving || savingGrandpa}
              >
                {saving || savingGrandpa ? '验证中...' : '确认'}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}
