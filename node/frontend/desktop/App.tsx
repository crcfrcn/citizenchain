import { NrcSection } from "../institution/NrcSection";
import { PrcSection } from "../institution/PrcSection";
import { PrbSection } from "../institution/PrbSection";
import { ClearingBankSection } from "../offchain/section";
import { HomeNodeSection, OtherTabsSection, SettingsSection } from "../control/index";
import { TransactionPanel } from "../onchain/TransactionPanel";
import { MiningDashboardSection } from "../mining/index";
import { settingsApi } from "../control/api";
import { shouldShowDesktopUpdateDot } from "../control/updateIndicator";
import type { DesktopInstaller, DesktopUpdateInfo, DesktopUpdateProgress } from "../control/types";
// 节点 UI 顶级 tab 栏：首页 / 挖矿 / 国家储委会 / 省储委会 / 省储行 / 清算行 / 白皮书 / 公民宪法 / 设置。
import { useCallback, useEffect, useState } from 'react';

type TabKey =
  | 'home'
  | 'mining'
  | 'nrc'
  | 'prc'
  | 'prb'
  | 'clearing-bank'
  | 'whitepaper'
  | 'constitution'
  | 'settings';

export default function App() {
  const [tab, setTab] = useState<TabKey>('home');
  const [desktopUpdate, setDesktopUpdate] = useState<DesktopInstaller | null>(null);
  const [desktopUpdateInfo, setDesktopUpdateInfo] = useState<DesktopUpdateInfo>({
    status: 'checking',
    currentVersion: null,
    latestVersion: null,
    error: null,
  });

  const applyUpdateProgress = useCallback((progress: DesktopUpdateProgress) => {
    if (progress.status === 'idle') return;
    setDesktopUpdateInfo(prev => ({ ...prev, status: progress.status === 'idle' ? prev.status : progress.status,
      latestVersion: progress.version ?? prev.latestVersion, downloaded: progress.downloaded,
      total: progress.total, error: progress.error }));
  }, []);

  const checkDesktopUpdate = useCallback(async () => {
    setDesktopUpdateInfo((prev) => ({ ...prev, status: 'checking', error: null }));
    try {
      // App 打开后只检查 GitHub Release 元数据，不下载、不安装，等待用户在设置页主动点击。
      const cached = await settingsApi.desktopUpdateStatus();
      if (['ready', 'downloading', 'installing'].includes(cached.status)) {
        applyUpdateProgress(cached);
        return;
      }
      const metadata = await settingsApi.checkDesktopUpdate();
      const update = metadata;
      setDesktopUpdate(update);
      setDesktopUpdateInfo({
        status: update ? 'available' : 'unavailable',
        currentVersion: update?.currentVersion ?? null,
        latestVersion: update?.version ?? null,
        error: null,
      });
    } catch (error) {
      setDesktopUpdate(null);
      setDesktopUpdateInfo({
        status: 'error',
        currentVersion: null,
        latestVersion: null,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }, [applyUpdateProgress]);

  useEffect(() => {
    void checkDesktopUpdate();
  }, [checkDesktopUpdate]);

  const updateDesktop = useCallback(async () => {
    if (desktopUpdateInfo.status === 'downloading' || desktopUpdateInfo.status === 'installing') return;
    if (!desktopUpdate && !desktopUpdateInfo.latestVersion && desktopUpdateInfo.status !== 'ready') return;
    const installing = desktopUpdateInfo.status === 'ready';
    setDesktopUpdateInfo(prev => ({ ...prev, status: installing ? 'installing' : 'downloading', error: null }));
    try {
      if (installing) await settingsApi.installDesktopUpdate();
      else await settingsApi.downloadDesktopUpdate(applyUpdateProgress);
    } catch (error) {
      if (installing) {
        try {
          const status = await settingsApi.desktopUpdateStatus();
          if (status.status === 'error') { applyUpdateProgress(status); return; }
        } catch { /* 读取状态失败仍保留安装错误，允许重试。 */ }
      }
      setDesktopUpdateInfo(prev => ({ ...prev, status: installing ? 'ready' : 'error',
        error: error instanceof Error ? error.message : String(error) }));
    }
  }, [desktopUpdate, desktopUpdateInfo.status, desktopUpdateInfo.latestVersion, applyUpdateProgress]);

  const showSettingsUpdateDot = shouldShowDesktopUpdateDot(desktopUpdateInfo);

  return (
    <div className="page">
      <nav className="top-nav">
        <button className={tab === 'home' ? 'active' : ''} onClick={() => setTab('home')}>首页</button>
        <button className={tab === 'mining' ? 'active' : ''} onClick={() => setTab('mining')}>挖矿</button>
        <button className={tab === 'nrc' ? 'active' : ''} onClick={() => setTab('nrc')}>国家储委会</button>
        <button className={tab === 'prc' ? 'active' : ''} onClick={() => setTab('prc')}>省储委会</button>
        <button className={tab === 'prb' ? 'active' : ''} onClick={() => setTab('prb')}>省储行</button>
        <button className={tab === 'clearing-bank' ? 'active' : ''} onClick={() => setTab('clearing-bank')}>清算行</button>
        <button className={tab === 'whitepaper' ? 'active' : ''} onClick={() => setTab('whitepaper')}>白皮书</button>
        <button className={tab === 'constitution' ? 'active' : ''} onClick={() => setTab('constitution')}>公民宪法</button>
        <button
          className={`top-nav-settings-button ${tab === 'settings' ? 'active' : ''}`}
          onClick={() => setTab('settings')}
        >
          设置
          {showSettingsUpdateDot ? (
            <span className="top-nav-update-dot" aria-label="有更新" />
          ) : null}
        </button>
      </nav>

      {tab === 'home' ? (
        <div className="home-dual-panel">
          <main className="app">
            <section className="content">
              <HomeNodeSection />
            </section>
          </main>
          <aside className="app">
            <section className="content">
              <TransactionPanel />
            </section>
          </aside>
        </div>
      ) : (
        <main className="app">
          <section className="content">
            {tab === 'settings' ? (
              <SettingsSection
                desktopUpdateInfo={desktopUpdateInfo}
                onUpdateDesktop={updateDesktop}
              />
            ) : null}

            {tab === 'mining' ? (
              <MiningDashboardSection />
            ) : null}

            {tab === 'nrc' ? (
              <NrcSection />
            ) : null}

            {tab === 'prc' ? (
              <PrcSection />
            ) : null}

            {tab === 'prb' ? (
              <PrbSection />
            ) : null}

            {tab === 'clearing-bank' ? (
              <ClearingBankSection />
            ) : null}

            {tab === 'whitepaper' ? (
              <OtherTabsSection activeKey="whitepaper" />
            ) : null}

            {tab === 'constitution' ? (
              <OtherTabsSection activeKey="constitution" />
            ) : null}
          </section>
        </main>
      )}
    </div>
  );
}
