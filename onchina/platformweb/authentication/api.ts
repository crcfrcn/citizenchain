// 登录、登出、会话校验与二维码登录 API。
// 通用 HTTP 能力在 workspace/http.ts;本文件只放 auth 模块自己的后端接口。

import { adminHeaders, request } from "../workspace/http";
import type { AdminAuth } from './types';
import type { CapabilitySet } from '../workspace/modules';
import type { InstitutionWorkspace } from '../workspace/types';

export type AdminAuthCheck = {
  ok: boolean;
  accountId: string;
  institutionCidNumber: string;
  institutionCode: string;
  adminLevel?: string | null;
  capabilities?: CapabilitySet;
  workspace?: InstitutionWorkspace;
  familyName: string;
  givenName: string;
  scopeProvinceName?: string | null;
  scopeCityName?: string | null;
  scopeTownName?: string | null;
  cidShortName?: string | null;
};

export type AdminIdentifyResult = {
  accountId: string;
  institutionCidNumber: string;
  institutionCode: string;
  adminLevel?: string | null;
  capabilities?: CapabilitySet;
  workspace?: InstitutionWorkspace;
  familyName: string;
  givenName: string;
  scopeProvinceName?: string | null;
  scopeCityName?: string | null;
  scopeTownName?: string | null;
  cidShortName?: string | null;
};

export type AdminQrSignRequestResult = {
  challengeId: string;
  challengePayload: string;
  loginQrPayload: string;
  origin: string;
  domain: string;
  sessionId: string;
  expiresAt: number;
};

export type AdminVerifyResult = {
  accessToken: string;
  expiresAt: number;
  admin: AdminIdentifyResult;
};

export type AdminInstitutionCandidate = {
  candidateId: string;
  institutionCode: string;
  adminLevel?: string | null;
  institutionCidNumber?: string | null;
  frgProvinceCode?: string | null;
  cidFullName?: string | null;
  cidShortName?: string | null;
  scopeProvinceName?: string | null;
  scopeCityName?: string | null;
  scopeTownName?: string | null;
};

export type NodeBindingRequired = {
  bindingChallengeId: string;
  accountId: string;
  candidates: AdminInstitutionCandidate[];
};

export type AdminLoginCompleteResult = {
  status: 'SUCCESS' | 'BINDING_REQUIRED';
  accessToken?: string;
  expiresAt?: number;
  admin?: AdminIdentifyResult;
  binding?: NodeBindingRequired;
};

export type AdminQrLoginStatus = {
  status: 'PENDING' | 'SUCCESS' | 'EXPIRED';
  message: string;
  accessToken?: string;
  expiresAt?: number;
  admin?: AdminIdentifyResult;
};

export async function createAdminQrSignRequest(input: {
  identityQr: string;
  origin: string;
  sessionId: string;
}): Promise<AdminQrSignRequestResult> {
  return request<AdminQrSignRequestResult>('/api/admin/auth/qr/sign-request', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input),
  });
}

export async function queryAdminQrLoginResult(
  challengeId: string,
  sessionId: string,
): Promise<AdminQrLoginStatus> {
  const q = `?challenge_id=${encodeURIComponent(challengeId)}&session_id=${encodeURIComponent(sessionId)}`;
  return request<AdminQrLoginStatus>(`/api/admin/auth/qr/result${q}`, {
    method: 'GET',
  });
}

export async function completeAdminQrLogin(input: {
  challengeId: string;
  sessionId?: string;
  accountId: string;
  signature: string;
}): Promise<AdminLoginCompleteResult> {
  return request<AdminLoginCompleteResult>('/api/admin/auth/qr/complete', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input),
  });
}

export async function confirmNodeBinding(input: {
  bindingChallengeId: string;
  candidateId: string;
}): Promise<AdminVerifyResult> {
  return request<AdminVerifyResult>('/api/admin/auth/node-binding/confirm', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input),
  });
}

/** 主动登出:通知后端销毁 session。best-effort,不阻塞前端退出流程。 */
export async function adminLogout(auth: AdminAuth): Promise<void> {
  try {
    await request<string>('/api/admin/auth/logout', {
      method: 'POST',
      headers: adminHeaders(auth),
    });
  } catch {
    // 静默:即使后端不可达也不影响前端退出。
  }
}

export async function checkAdminAuth(auth: AdminAuth): Promise<AdminAuthCheck> {
  return request<AdminAuthCheck>('/api/admin/auth/check', {
    headers: adminHeaders(auth),
  });
}
