// 节点 UI 顶级 tab 栏：首页 / 挖矿 / 国家储委会 / 省储委会 / 省储行 / 清算行 / 白皮书 / 公民宪法 / 设置。
import { useCallback, useEffect, useState } from 'react';
import { relaunch } from '@tauri-apps/plugin-process';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { NrcSection } from '../governance/NrcSection';
import { PrcSection } from '../governance/PrcSection';
import { PrbSection } from '../governance/PrbSection';
import { ClearingBankSection } from '../transaction/offchain/section';
import { HomeNodeSection } from '../home';
import { TransactionPanel } from '../transaction/onchain/TransactionPanel';
import { MiningDashboardSection } from '../mining';
import { OtherTabsSection } from '../other-tabs';
import { settingsApi } from '../settings/api';
import { SettingsSection } from '../settings/settings-panel';
import { shouldShowDesktopUpdateDot } from '../settings/updateIndicator';
import type { DesktopUpdateInfo } from '../settings/types';

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
  const [desktopUpdate, setDesktopUpdate] = useState<Update | null>(null);
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
      const update = await check();
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

  const installDesktopUpdate = useCallback(async () => {
    if (!desktopUpdate) return;
    setDesktopUpdateInfo((prev) => ({ ...prev, status: 'installing', error: null }));
    try {
      await settingsApi.prepareDesktopUpdate();
      await desktopUpdate.downloadAndInstall();
      await relaunch();
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
                onInstallDesktopUpdate={installDesktopUpdate}
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
