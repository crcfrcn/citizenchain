// 在sessionStorage保存和校验管理员公开登录态。

import type { AdminAuth } from './types';

// 登录态只保留当前严格形状（完整会话上下文及setupRequired）；
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
      typeof parsed.walletRevision === 'string' && /^(0|[1-9][0-9]*)$/.test(parsed.walletRevision) &&
      BigInt(parsed.walletRevision) <= 18446744073709551615n &&
      (parsed.walletNamespace === 'PublicAdmins' || parsed.walletNamespace === 'PrivateAdmins') &&
      typeof parsed.institutionCidNumber === 'string' && parsed.institutionCidNumber.length > 0 &&
      typeof parsed.setupRequired === 'boolean' &&
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
