import React from 'react';
import ReactDOM from 'react-dom/client';
import { ConfigProvider } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import App from './App';
import { DisplayScreen } from './legislation/display/DisplayScreen';
import ErrorBoundary from './core/ErrorBoundary';
import { cidTheme } from './theme';
import 'antd/dist/reset.css';

// 大屏只读入口经 hash `#/display` 顶层分流——绕过 AuthProvider/App(免登录只读),
// 契合 ADR-030 operator/display 路由分离。其余一切走原管理端 App。
const isDisplay = window.location.hash.startsWith('#/display');

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <ConfigProvider locale={zhCN} theme={cidTheme}>
      <ErrorBoundary>{isDisplay ? <DisplayScreen /> : <App />}</ErrorBoundary>
    </ConfigProvider>
  </React.StrictMode>
);
