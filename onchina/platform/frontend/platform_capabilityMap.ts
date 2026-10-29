// 控制台能力位类型 + 空能力兜底。
//
// 能力由后端各模块真实入口声明，经 platform/modules/capability.rs 汇总并由会话下发。
// 前端只镜像渲染工作台入口,不在此硬编码权限——render-gating 非安全边界,后端始终对越权独立拒绝。
// 业务能力由模块真实声明提供，公共 capability.rs 负责汇总；前端不另写权限。

export type CapabilitySet = {
  canViewCitizens: boolean;
  canViewInstitutions: boolean;
  canViewPrivate: boolean;
  canViewEducation: boolean;
  canViewFederalRegistryAdmins: boolean;
  canViewCityRegistryAdmins: boolean;
  canCrudCityRegistryAdmins: boolean;
  /** 只读「本机构管理员」位:非注册局法人可查看本机构链上管理员列表(只读)。 */
  canViewOwnAdmins: boolean;
  canManageInstitutions: boolean;
  canRegisterInstitutions: boolean;
  canBusinessWrite: boolean;
  canViewCityRegistry: boolean;
  canViewFederalRegistry: boolean;
  /** 立法:查看立法/提案/大屏(立法机构通用只读位)。 */
  canViewLegislation: boolean;
  /** 立法:发起法律案(发起院/教委会/自治会;参议会无此位)。 */
  canProposeLegislation: boolean;
  /** 立法：当前代表机构表决；仅提案机构无此能力。 */
  canCastRepresentativeVote: boolean;
  /** 立法:行政签署/三人会签/护宪终审(另线程接入,本轮恒 false)。 */
  canSignLegislation: boolean;
  /** 立法:发起任免案(政府;Phase 4 接入,本轮恒 false)。 */
  canProposePersonnel: boolean;
  /** 立法:发起预算案(政府;Phase 4 接入,本轮恒 false)。 */
  canProposeBudget: boolean;
};

/** 空能力:未登录、能力未下发或未知机构码时的兜底(不显示任何受限 tab)。 */
export const EMPTY_CAPABILITIES: CapabilitySet = {
  canViewCitizens: false,
  canViewInstitutions: false,
  canViewPrivate: false,
  canViewEducation: false,
  canViewFederalRegistryAdmins: false,
  canViewCityRegistryAdmins: false,
  canCrudCityRegistryAdmins: false,
  canViewOwnAdmins: false,
  canManageInstitutions: false,
  canRegisterInstitutions: false,
  canBusinessWrite: false,
  canViewCityRegistry: false,
  canViewFederalRegistry: false,
  canViewLegislation: false,
  canProposeLegislation: false,
  canCastRepresentativeVote: false,
  canSignLegislation: false,
  canProposePersonnel: false,
  canProposeBudget: false,
};


// 模块只更换装载方式；React、登录Context和请求状态由同一宿主实例提供。
import type { ComponentType } from 'react';
import type { AdminAuth } from './authentication_types';
import { request } from './helpers_http';
export type ModuleAsset = {
  id: string; version: string; interface_version: number; script: string; stylesheet: string;
  exports: string[]; institution_codes: string[]; workspace: string | null;
};
export type ModuleMetadata = Pick<ModuleAsset, 'id' | 'version' | 'interface_version'>;
export type LoadedModule = Record<string, unknown> & { __onchina?: ModuleMetadata };
// 全局声明随实际公开接口进入浏览器与测试工程，不依赖编辑器隐式发现d.ts。
declare global {
  var OnChinaPlatform: Readonly<Record<string, unknown>>;
  var OnChinaModules: Record<string, LoadedModule>;
}
let assets: ModuleAsset[] = [];
let initialization: Promise<void> | null = null;
let initializationError: Error | null = null;
const failures = new Map<string, Error>();
function sameOriginAsset(value: string, id: string): string {
  const url = new URL(value, window.location.href);
  if (url.origin !== window.location.origin || !url.pathname.startsWith('/modules/' + id + '/') || url.username || url.password) {
    throw new Error('模块资源地址无效');
  }
  return url.href;
}
// 装载只等待有限时间；失败保留隔离结果，恢复仍需原有页面刷新。
const MODULE_LOAD_TIMEOUT_MS = 15000;
async function loadModule(asset: ModuleAsset): Promise<void> {
  if (!/^[a-z][a-z0-9_]{0,63}$/.test(asset.id) || !/^\d+\.\d+\.\d+$/.test(asset.version)
      || asset.interface_version !== 1 || !Array.isArray(asset.exports) || !Array.isArray(asset.institution_codes)) {
    throw new Error('模块声明无效或接口版本不匹配');
  }
  // 两个地址都校验完成才创建节点，失败不能留下半套资源。
  const stylesheetUrl = sameOriginAsset(asset.stylesheet, asset.id);
  const scriptUrl = sameOriginAsset(asset.script, asset.id);
  const stylesheet = document.createElement('link');
  stylesheet.rel = 'stylesheet'; stylesheet.href = stylesheetUrl;
  const script = document.createElement('script');
  script.src = scriptUrl; script.async = true;
  let settled = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const cleanup = () => {
    clearTimeout(timer);
    stylesheet.onload = stylesheet.onerror = script.onload = script.onerror = null;
  };
  try {
    await new Promise<void>((resolve, reject) => {
      let styleReady = false;
      let scriptReady = false;
      const fail = (message: string) => {
        if (settled) return;
        settled = true; cleanup(); reject(new Error(message));
      };
      const ready = () => {
        if (settled || !styleReady || !scriptReady) return;
        settled = true; cleanup(); resolve();
      };
      stylesheet.onload = () => { styleReady = true; ready(); };
      stylesheet.onerror = () => fail('模块样式装载失败');
      script.onload = () => {
        if (settled) return;
        const module = globalThis.OnChinaModules[asset.id]; const metadata = module?.__onchina;
        if (!metadata || metadata.id !== asset.id || metadata.version !== asset.version
            || metadata.interface_version !== asset.interface_version
            || asset.exports.some(name => !Object.prototype.hasOwnProperty.call(module, name))) {
          fail('模块后端与前端声明不匹配'); return;
        }
        scriptReady = true; ready();
      };
      script.onerror = () => fail('模块脚本装载失败');
      timer = setTimeout(() => fail('模块资源装载超时'), MODULE_LOAD_TIMEOUT_MS);
      document.head.append(stylesheet, script);
    });
  } catch (error) {
    settled = true; cleanup(); stylesheet.remove(); script.remove();
    delete globalThis.OnChinaModules[asset.id];
    // 已开始下载的脚本可能在移除节点后才执行；本次页面禁止迟到写回，刷新后自然重建。
    Object.defineProperty(globalThis.OnChinaModules, asset.id, { configurable: true, get: () => undefined, set: () => {} });
    throw error;
  }
}
/** 在原启动阶段装载一次；运行期间不热替换模块。 */
export function initializeModules(): Promise<void> {
  if (initialization) return initialization;
  initialization = (async () => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), MODULE_LOAD_TIMEOUT_MS);
    const health = await request<{ modules: ModuleAsset[] }>('/api/health', { signal: controller.signal })
      .finally(() => clearTimeout(timer));
    if (!Array.isArray(health.modules)) throw new Error('平台未返回模块声明');
    assets = health.modules;
    const ids = new Set<string>();
    await Promise.all(assets.map(async asset => {
      try {
        if (ids.has(asset.id)) throw new Error('重复模块声明');
        ids.add(asset.id); await loadModule(asset);
      } catch (error) { failures.set(asset.id, error instanceof Error ? error : new Error('模块装载失败')); }
    }));
  })().catch(error => {
    initializationError = error instanceof Error ? error : new Error('模块装载失败'); throw initializationError;
  });
  return initialization;
}
export function moduleView<Props extends object>(id: string, name: string): ComponentType<Props> {
  const failure = failures.get(id) ?? initializationError; if (failure) throw failure;
  const value = globalThis.OnChinaModules[id]?.[name];
  if (typeof value !== 'function') throw new Error('所需模块未装载');
  return value as ComponentType<Props>;
}
/** 已验证会话的实例级模块优先，其次使用模块声明的机构类工作台。 */
export function workspaceView<Props extends object>(auth: AdminAuth): ComponentType<Props> | null {
  const instance = assets.filter(asset => asset.workspace && auth.workspace?.workspace_modules.includes(asset.id));
  const owned = instance.length ? instance : assets.filter(asset => asset.workspace && asset.institution_codes.includes(auth.institutionCode));
  if (owned.length > 1) throw new Error('机构工作台模块归属冲突');
  const asset = owned[0];
  return asset?.workspace ? moduleView<Props>(asset.id, asset.workspace) : null;
}
