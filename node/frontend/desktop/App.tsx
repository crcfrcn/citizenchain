import { NrcSection } from "../institution/NrcSection";
import { PrcSection } from "../institution/PrcSection";
import { PrbSection } from "../institution/PrbSection";
import { ClearingBankSection } from "../offchain/section";
import { HomeNodeSection, OtherTabsSection, SettingsSection } from "../control/index";
import { TransactionPanel } from "../onchain/TransactionPanel";
import { MiningDashboardSection } from "../mining/index";
import { settingsApi } from "../control/api";
import { shouldShowDesktopUpdateDot } from "../control/updateIndicator";
import type { DesktopInstaller, DesktopUpdateInfo } from "../control/types";
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

  const checkDesktopUpdate = useCallback(async () => {
    setDesktopUpdateInfo((prev) => ({ ...prev, status: 'checking', error: null }));
    try {
      // App 打开后只检查 GitHub Release 元数据，不下载、不安装，等待用户在设置页主动点击。
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
  }, []);

  useEffect(() => {
    void checkDesktopUpdate();
  }, [checkDesktopUpdate]);

  const openDesktopInstaller = useCallback(async () => {
    if (!desktopUpdate) return;
    setDesktopUpdateInfo((prev) => ({ ...prev, error: null }));
    try {
      // 下载完整包由系统浏览器承接；此动作不停止节点、不安装或重启应用。
      await settingsApi.openDesktopInstaller();
    } catch (error) {
      setDesktopUpdateInfo((prev) => ({
        ...prev,
        status: 'available',
        error: error instanceof Error ? error.message : String(error),
      }));
    }
  }, [desktopUpdate]);

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
                onOpenDesktopInstaller={openDesktopInstaller}
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
