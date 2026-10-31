// cid 前端管理员登录态持久化工具。
// 从 App.tsx 里抽出来的 readStoredAuth / writeStoredAuth / clearStoredAuth,
// 使用 sessionStorage 与既有结构校验，由 AuthContext 维护登录态。

import type { AdminAuth } from './types';

// 登录态只保留当前严格形状（institution_code + admin_level + scope_town_name）；
// 解析失败的历史形状一律丢弃，避免读出空字段或形成兼容分支。
const AUTH_STORAGE_KEY = 'onchina_admin_auth';

export function readStoredAuth(): AdminAuth | null {
  try {
    const raw = sessionStorage.getItem(AUTH_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<AdminAuth>;
    if (
      typeof parsed === 'object' &&
      parsed &&
      'accessToken' in parsed &&
      typeof parsed.accessToken === 'string' &&
      typeof parsed.accountId === 'string' &&
      typeof parsed.institutionCode === 'string'
    ) {
      return parsed as AdminAuth;
    }
    return null;
  } catch {
    return null;
  }
}

export function writeStoredAuth(auth: AdminAuth) {
  try {
    sessionStorage.setItem(AUTH_STORAGE_KEY, JSON.stringify(auth));
  } catch {
    // 静默:隐身模式或 sessionStorage 不可用时不阻塞流程
  }
}

export function clearStoredAuth() {
  try {
    sessionStorage.removeItem(AUTH_STORAGE_KEY);
  } catch {
    // 静默
  }
}
