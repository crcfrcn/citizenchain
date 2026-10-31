import { adminsChangeApi } from "../institution/admins";
import { homeNodeApi, settingsApi } from "./api";
import { RewardAccountSection } from "./RewardAccountSection";
import { NodeModeSection } from "./NodeModeSection";
import { NodeKeySection } from "./NodeKeySection";
import { OnChinaPlatformSection } from "./OnChinaPlatformSection";
import type { ChainStatus, BootnodeKey, DesktopUpdateInfo, NodeModeState, OnChinaPlatformState, RewardAccount } from "./types";
import { useCallback, useEffect, useState } from 'react';

type SettingsSectionProps = {
  desktopUpdateInfo: DesktopUpdateInfo;
  onUpdateDesktop: () => Promise<void>;
};

export function SettingsSection({
  desktopUpdateInfo,
  onUpdateDesktop,
}: SettingsSectionProps) {
  const [nodeMode, setNodeMode] = useState<NodeModeState | null>(null);
  const [onChinaPlatform, setOnChinaPlatform] =
    useState<OnChinaPlatformState | null>(null);
  const [rewardAccount, setRewardAccount] = useState<RewardAccount>({
    accountId: null,
  });
  const [nodeKey, setNodeKey] = useState<BootnodeKey>({
    nodeKey: null,
    peerId: null,
    cidShortName: null,
  });
  const [chainStatus, setChainStatus] = useState<ChainStatus | null>(null);
  const [isAdmin, setIsAdmin] = useState(false);

  const loadSettings = useCallback(async () => {
    const [m, p, w, k, c, a] = await Promise.allSettled([
      settingsApi.getNodeMode(),
      settingsApi.getOnChinaPlatform(),
      settingsApi.getRewardAccount(),
      settingsApi.getBootnodeKey(),
      homeNodeApi.getChainStatus(),
      adminsChangeApi.hasAnyActivatedAdmin(),
    ]);
    if (m.status === 'fulfilled') setNodeMode(m.value);
    if (p.status === 'fulfilled') setOnChinaPlatform(p.value);
    if (w.status === 'fulfilled') setRewardAccount(w.value);
    if (k.status === 'fulfilled') setNodeKey(k.value);
    if (c.status === 'fulfilled') setChainStatus(c.value);
    if (a.status === 'fulfilled') setIsAdmin(a.value);
  }, []);

  useEffect(() => {
    void loadSettings().catch(() => undefined);
  }, [loadSettings]);

  return (
    <>
      <NodeModeSection nodeMode={nodeMode} onUpdated={setNodeMode} />
      <OnChinaPlatformSection
        platform={onChinaPlatform}
        onUpdated={setOnChinaPlatform}
      />
      <RewardAccountSection
        rewardAccount={rewardAccount}
        onUpdated={setRewardAccount}
      />
      {isAdmin && (
        <NodeKeySection
          nodeKey={nodeKey}
          onUpdated={setNodeKey}
          onApplied={() => {
            void loadSettings();
          }}
        />
      )}
      {chainStatus && (
        <div className="settings-version-section">
          <div className="settings-version-row">
            <span className="settings-version-label">节点程序版本</span>
            <span className="settings-version-value-wrap">
              <span className="settings-version-value">{chainStatus.nodeVersion}</span>
              {['downloading', 'ready', 'installing'].includes(desktopUpdateInfo.status) && (
                <span className="settings-download-progress">
                  <progress aria-label="新版下载进度" max={desktopUpdateInfo.total || 1}
                    value={desktopUpdateInfo.downloaded || 0} />
                  <span>{desktopUpdateInfo.total
                    ? `${Math.floor((desktopUpdateInfo.downloaded || 0) * 100 / desktopUpdateInfo.total)}%` : '0%'}</span>
                </span>
              )}
              {['available', 'downloading', 'ready', 'installing'].includes(desktopUpdateInfo.status)
                || (desktopUpdateInfo.status === 'error' && desktopUpdateInfo.latestVersion) ? (
                <button type="button" className="settings-update-button"
                  disabled={desktopUpdateInfo.status === 'downloading' || desktopUpdateInfo.status === 'installing'}
                  title={desktopUpdateInfo.latestVersion ? `更新到 ${desktopUpdateInfo.latestVersion}` : undefined}
                  onClick={() => { void onUpdateDesktop(); }}>
                  {desktopUpdateInfo.status === 'downloading' ? '下载中'
                    : desktopUpdateInfo.status === 'installing' ? '安装中'
                    : desktopUpdateInfo.status === 'error' ? '重试下载' : '安装更新'}
                </button>
              ) : null}
            </span>
          </div>
          {desktopUpdateInfo.status === 'ready' && <p>下载完成，点击安装更新后将退出当前程序，安装并启动新版。</p>}
          {desktopUpdateInfo.error ? <p role="alert">{desktopUpdateInfo.error}</p> : null}
          <div className="settings-version-row">
            <span className="settings-version-label">Runtime 版本</span>
            <span className="settings-version-value">
              {chainStatus.specVersion != null ? `spec ${chainStatus.specVersion}` : '节点未运行'}
            </span>
          </div>
        </div>
      )}
    </>
  );
}
