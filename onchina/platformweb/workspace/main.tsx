import * as SharedSDK from './index';
import React from 'react';
import ReactDOM from 'react-dom/client';
import { ConfigProvider } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import App from './App';
import { initializeModules, moduleView } from "./modules";
// 模块构建外置这些公开导出，登录、扫码和401状态仍只有同一份实现。
globalThis.OnChinaModules = Object.create(null) as Record<string, import("./modules").LoadedModule>;
globalThis.OnChinaPlatform = Object.freeze({ ...SharedSDK });
const DisplayScreen = () => { const View = moduleView<Record<string, never>>('legislation', 'DisplayScreen'); return <View />; };
import ErrorBoundary from "../controls/ErrorBoundary";
import { cidTheme } from "../controls/theme";
import 'antd/dist/reset.css';

// 大屏只读入口经 hash `#/display` 顶层分流——绕过 AuthProvider/App(免登录只读),
// 契合 ADR-030 operator/display 路由分离。其余一切走原管理端 App。
const isDisplay = window.location.hash.startsWith('#/display');

const root = ReactDOM.createRoot(document.getElementById('root')!);
const render = () => root.render(
  <React.StrictMode>
    <ConfigProvider locale={zhCN} theme={cidTheme}>
      <ErrorBoundary>{isDisplay ? <DisplayScreen /> : <App />}</ErrorBoundary>
    </ConfigProvider>
  </React.StrictMode>
);

if (isDisplay) { void initializeModules().catch(() => undefined).then(render); }
else { render(); }
