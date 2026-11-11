import { sanitizeError } from "../client/tauri";
import { MetricHelp } from "../desktop/MetricHelp";
import { settingsApi as api } from "./api";
import type { OnChinaModuleId, OnChinaPlatformState } from "./types";
import { useEffect, useState } from 'react';

const modules: { id: OnChinaModuleId; title: string; description: string; icon: string }[] = [
  { id: 'registry', title: '注册局', description: '公民和机构登记、资料管理', icon: 'M3 5h18v14H3z M7 14c0-3 6-3 6 0 M10 8a1.5 1.5 0 1 0 0 3a1.5 1.5 0 0 0 0-3 M16 10h2 M16 14h2' },
  { id: 'legislation', title: '立法院', description: '查看立法提案，按权限发起或表决', icon: 'M6 3h8l4 4v14H6z M14 3v5h4 M9 12h6 M9 16h6' },
  { id: 'judicial', title: '司法院', description: '司法院工作台、机构资料和管理员', icon: 'M3 8l9-5 9 5z M5 11v7 M10 11v7 M14 11v7 M19 11v7 M3 21h18' },
  { id: 'private', title: '私权机构', description: '本机构资料及已授权业务', icon: 'M4 21V4h11v17 M15 10h5v11 M2 21h20 M8 8h3 M8 12h3 M8 16h3' },
];

const REGISTRY_HELP = [
  '身份注册局由1个联邦注册局节点+每个市一个市注册局节点组成，且平行去中心化。',
  '注册局负责公民身份注册和机构身份注册，机构身份为公权、私权、盈利、非盈利等。',
  '首次登记链上投票身份的公民，将获得认证奖励，奖励由公民发行模块发行公民币奖励。',
  '可获得奖励的总量为1,443,497,378个，前14,436,417个登记的公民，每人奖励9999.00元。',
  '第14,436,417个之后登记的公民，每人奖励999.00元，超过奖励总量后永久停止发行。',
  '登记链上投票公民身份指在任意注册局注册公民身份，且投票身份必须公开选区、身份状态和有效期。',
];

export function OnChinaPlatformSection() {
  const [platform, setPlatform] = useState<OnChinaPlatformState | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  const [pendingAction, setPendingAction] = useState<'start' | 'stop' | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const running = platform?.running ?? false;
  const status = platform?.status ?? 'stopped';
  const statusLabel = platform?.statusLabel ?? '未开启';
  // 仅省略显示用的根路径斜杠，连接地址仍由宿主统一声明。
  const url = platform?.url?.replace(/\/$/u, '') || '入口加载中';
  const actionLabel = saving ? '处理中' : running ? '关闭链上中国' : '启动链上中国';

  useEffect(() => {
    // 启停期间停掉轮询，避免旧状态覆盖启停结果。
    if (saving) return;
    let cancelled = false;
    let timer: number | undefined;
    const refresh = async () => {
      try {
        const next = await api.getOnChinaPlatform();
        if (cancelled) return;
        setPlatform(next);
        setStatusError(null);
        if (!next.running && next.status !== 'error') return;
      } catch (e) {
        if (cancelled) return;
        setPlatform(null);
        setStatusError(sanitizeError(e));
      }
      timer = window.setTimeout(refresh, 1500);
    };
    void refresh();
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [saving]);

  const confirmAction = async () => {
    if (!pendingAction) return;
    setSaving(true);
    setError(null);
    try {
      const next = pendingAction === 'start'
        ? await api.startOnChinaPlatform()
        : await api.stopOnChinaPlatform();
      setPlatform(next);
      setPendingAction(null);
    } catch (e) {
      setError(sanitizeError(e));
    } finally {
      setSaving(false);
    }
  };

  const openModule = async (moduleId: OnChinaModuleId) => {
    setOpening(true);
    setError(null);
    try {
      await api.openOnChinaModule(moduleId);
    } catch (e) {
      setError(sanitizeError(e));
    } finally {
      setOpening(false);
    }
  };

  return (
    <section className="onchina-platform-section">
      <div className="onchina-platform-row">
        <span className="onchina-platform-heading">
          <span
            className={`status-dot ${status === 'enabled' ? 'running' : 'stopped'}`}
            data-status={status}
            role="status"
            title={statusLabel}
            aria-label={statusLabel}
          />
          <span className="onchina-platform-label">链上中国平台</span>
        </span>
        <span className="onchina-platform-url">{url}</span>
        <button
          type="button"
          className={`node-lifecycle-button ${running ? 'stop' : 'start'}`}
          title={actionLabel}
          aria-label={actionLabel}
          disabled={!platform || saving}
          onClick={() => setPendingAction(running ? 'stop' : 'start')}
        />
      </div>
      {platform?.detail ? (
        <p className="section-inline-error">{platform.detail}</p>
      ) : null}
      {statusError ? <p role="alert" className="section-inline-error">{statusError}</p> : null}
      {error ? <p role="alert" className="section-inline-error">{error}</p> : null}
      <section className="onchina-modules" aria-labelledby="onchina-modules-title">
        <div className="onchina-modules-heading">
          <h2 id="onchina-modules-title">平台模块</h2>
          <p>{status === 'enabled'
            ? '点击模块，在系统默认浏览器扫码登录。'
            : '平台启动后，点击模块在系统默认浏览器扫码登录。'}</p>
        </div>
        <div className="onchina-module-grid">
          {modules.map(({ id, title, description, icon }) => (
            <div key={id} className="onchina-module">
              <button
                type="button"
                className="onchina-module-open"
                aria-label={`${title} ${description}`}
                disabled={status !== 'enabled' || !platform?.modules.includes(id) || saving || opening}
                onClick={() => void openModule(id)}
              />
              <span className="onchina-module-icon" aria-hidden="true">
                <svg viewBox="0 0 24 24"><path d={icon} /></svg>
              </span>
              <span className="onchina-module-copy">
                <span className="onchina-module-title">
                  <strong>{title}</strong>
                  {id === 'registry' && <MetricHelp title="身份注册局说明" items={REGISTRY_HELP} />}
                </span>
                <span>{description}</span>
              </span>
              <span aria-hidden="true" className="onchina-module-arrow">↗</span>
            </div>
          ))}
        </div>
      </section>
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
