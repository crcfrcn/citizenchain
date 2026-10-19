import React from 'react';
import ReactDOM from 'react-dom/client';
import { ConfigProvider } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import App from './App';
import * as ReactRuntime from 'react';
import * as ReactDOMRuntime from 'react-dom/client';
import * as JSXRuntime from 'react/jsx-runtime';
import * as JSXDevRuntime from 'react/jsx-dev-runtime';
import * as AntdRuntime from 'antd';
import * as IconRuntime from '@ant-design/icons';
import dayjs from 'dayjs';
import { initializeModules, moduleView } from './platform_capabilityMap';
import * as Shared_accounts_AccountList from "./accounts_AccountList";
import * as Shared_accounts_AccountManageSection from "./accounts_AccountManageSection";
import * as Shared_admins_InstitutionAssignmentCard from "./admins_InstitutionAssignmentCard";
import * as Shared_admins_RegistryAdminsView from "./admins_RegistryAdminsView";
import * as Shared_admins_adminUtils from "./admins_adminUtils";
import * as Shared_admins_api from "./admins_api";
import * as Shared_admins_securityApi from "./admins_securityApi";
import * as Shared_authentication_AuthContext from "./authentication_AuthContext";
import * as Shared_authentication_passkeyClient from "./authentication_passkeyClient";
import * as Shared_authentication_usePasskey from "./authentication_usePasskey";
import * as Shared_china_api from "./china_api";
import * as Shared_china_metaCache from "./china_metaCache";
import * as Shared_core_CitizenSignatureModal from "./core_CitizenSignatureModal";
import * as Shared_core_InstitutionDetailNavLayout from "./core_InstitutionDetailNavLayout";
import * as Shared_core_ScanAccountModal from "./core_ScanAccountModal";
import * as Shared_core_cardStyles from "./core_cardStyles";
import * as Shared_core_modalStack from "./core_modalStack";
import * as Shared_core_useChainSign from "./core_useChainSign";
import * as Shared_docs_DocsLibrary from "./docs_DocsLibrary";
import * as Shared_helpers_http from "./helpers_http";
import * as Shared_helpers_notice from "./helpers_notice";
import * as Shared_helpers_parseSignedPayload from "./helpers_parseSignedPayload";
import * as Shared_helpers_ss58 from "./helpers_ss58";
import * as Shared_hooks_useAuth from "./hooks_useAuth";
import * as Shared_hooks_useScope from "./hooks_useScope";
import * as Shared_institutions_OwnInstitutionInfoPanel from "./institutions_OwnInstitutionInfoPanel";
import * as Shared_platform_capabilityMap from "./platform_capabilityMap";
import * as Shared_platform_registryTier from "./platform_registryTier";
import * as Shared_subjects_LegalRepresentativePhoto from "./subjects_LegalRepresentativePhoto";
import * as Shared_subjects_api from "./subjects_api";
import * as Shared_subjects_institutionLabels from "./subjects_institutionLabels";
import * as Shared_subjects_labels from "./subjects_labels";
import * as Shared_workspace_WorkspaceShell from "./workspace_WorkspaceShell";
// 模块构建外置这些公开导出，登录、扫码和401状态仍只有同一份实现。
globalThis.OnChinaModules = Object.create(null) as Record<string, import('./platform_capabilityMap').LoadedModule>;
globalThis.OnChinaPlatform = Object.freeze({
  react: ReactRuntime, react_dom_client: ReactDOMRuntime,
  react_jsx_runtime: JSXRuntime, react_jsx_dev_runtime: JSXDevRuntime,
  antd: AntdRuntime, ant_design_icons: IconRuntime, dayjs,
  platform_accounts_AccountList: Shared_accounts_AccountList,
  platform_accounts_AccountManageSection: Shared_accounts_AccountManageSection,
  platform_admins_InstitutionAssignmentCard: Shared_admins_InstitutionAssignmentCard,
  platform_admins_RegistryAdminsView: Shared_admins_RegistryAdminsView,
  platform_admins_adminUtils: Shared_admins_adminUtils,
  platform_admins_api: Shared_admins_api,
  platform_admins_securityApi: Shared_admins_securityApi,
  platform_authentication_AuthContext: Shared_authentication_AuthContext,
  platform_authentication_passkeyClient: Shared_authentication_passkeyClient,
  platform_authentication_usePasskey: Shared_authentication_usePasskey,
  platform_china_api: Shared_china_api,
  platform_china_metaCache: Shared_china_metaCache,
  platform_core_CitizenSignatureModal: Shared_core_CitizenSignatureModal,
  platform_core_InstitutionDetailNavLayout: Shared_core_InstitutionDetailNavLayout,
  platform_core_ScanAccountModal: Shared_core_ScanAccountModal,
  platform_core_cardStyles: Shared_core_cardStyles,
  platform_core_modalStack: Shared_core_modalStack,
  platform_core_useChainSign: Shared_core_useChainSign,
  platform_docs_DocsLibrary: Shared_docs_DocsLibrary,
  platform_helpers_http: Shared_helpers_http,
  platform_helpers_notice: Shared_helpers_notice,
  platform_helpers_parseSignedPayload: Shared_helpers_parseSignedPayload,
  platform_helpers_ss58: Shared_helpers_ss58,
  platform_hooks_useAuth: Shared_hooks_useAuth,
  platform_hooks_useScope: Shared_hooks_useScope,
  platform_institutions_OwnInstitutionInfoPanel: Shared_institutions_OwnInstitutionInfoPanel,
  platform_platform_capabilityMap: Shared_platform_capabilityMap,
  platform_platform_registryTier: Shared_platform_registryTier,
  platform_subjects_LegalRepresentativePhoto: Shared_subjects_LegalRepresentativePhoto,
  platform_subjects_api: Shared_subjects_api,
  platform_subjects_institutionLabels: Shared_subjects_institutionLabels,
  platform_subjects_labels: Shared_subjects_labels,
  platform_workspace_WorkspaceShell: Shared_workspace_WorkspaceShell,
});
const DisplayScreen = () => { const View = moduleView<Record<string, never>>('legislation', 'DisplayScreen'); return <View />; };
import ErrorBoundary from './core_ErrorBoundary';
import { cidTheme } from './theme';
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
