import { sanitizeError } from "../client/tauri";
import { settingsApi as api } from "./api";
import type { OnChinaPlatformState } from "./types";
import { useEffect, useState } from 'react';

type Props = {
  platform: OnChinaPlatformState | null;
  onUpdated: (next: OnChinaPlatformState) => void;
};

export function OnChinaPlatformSection({ platform, onUpdated }: Props) {
  const [pendingAction, setPendingAction] = useState<'start' | 'stop' | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const running = platform?.running ?? false;
  const status = platform?.status ?? 'stopped';
  const statusLabel = platform?.statusLabel ?? '未开启';
  // 仅省略显示用的根路径斜杠，连接地址仍由宿主统一声明。
  const url = platform?.url?.replace(/\/$/u, '') || '入口加载中';
  const actionText = running ? '关闭' : '启动';

  useEffect(() => {
    // 运行期间持续核实 HTTPS 健康；证书失效或宿主退出时及时更新，不把进程存在当就绪。
    if (!running && status !== 'error') return;
    const timer = window.setInterval(async () => {
      try {
        onUpdated(await api.getOnChinaPlatform());
      } catch {
        // 设置页轮询只刷新状态;单次失败不打断用户当前操作。
      }
    }, 1500);
    return () => window.clearInterval(timer);
  }, [onUpdated, running, status]);

  const confirmAction = async () => {
    if (!pendingAction) return;
    setSaving(true);
    setError(null);
    try {
      const next = pendingAction === 'start'
        ? await api.startOnChinaPlatform()
        : await api.stopOnChinaPlatform();
      onUpdated(next);
      setPendingAction(null);
    } catch (e) {
      setError(sanitizeError(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="section settings-onchina-platform-section">
      <div className="onchina-platform-row">
        <span className="onchina-platform-heading">
          <span className="onchina-platform-label">链上中国平台</span>
          <span className="onchina-platform-status" data-status={status} role="status">
            {statusLabel}
          </span>
        </span>
        <span className="onchina-platform-url">{url}</span>
        <button
          type="button"
          className="onchina-platform-button"
          disabled={!platform || saving}
          onClick={() => setPendingAction(running ? 'stop' : 'start')}
        >
          {saving ? '处理中' : actionText}
        </button>
      </div>
      {platform?.detail ? (
        <p className="section-inline-error">{platform.detail}</p>
      ) : null}
      {error ? <p className="section-inline-error">{error}</p> : null}
      {pendingAction ? (
        <div className="settings-confirm-mask" onClick={() => !saving && setPendingAction(null)}>
          <div className="settings-confirm" onClick={(event) => event.stopPropagation()}>
            <h3>确定</h3>
            <p>{pendingAction === 'start' ? '确认启动链上中国平台？' : '确认关闭链上中国平台？'}</p>
            <div className="settings-confirm-actions">
              <button type="button" disabled={saving} onClick={() => setPendingAction(null)}>
                取消
              </button>
              <button type="button" disabled={saving} onClick={confirmAction}>
                确认
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}
