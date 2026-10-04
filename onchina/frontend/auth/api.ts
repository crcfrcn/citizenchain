// 登录、登出、会话校验与二维码登录 API。
// 通用 HTTP 能力在 utils/http.ts;本文件只放 auth 模块自己的后端接口。

import { adminHeaders, request } from '../utils/http';
import type { AdminAuth } from './types';
import type { CapabilitySet } from '../platform/capabilityMap';
import type { InstitutionWorkspace } from '../workspace/types';

export type AdminAuthCheck = {
  ok: boolean;
  account_id: string;
  institution_cid_number: string;
  institution_code: string;
  admin_level?: string | null;
  capabilities?: CapabilitySet;
  workspace?: InstitutionWorkspace;
  family_name: string;
  given_name: string;
  scope_province_name?: string | null;
  scope_city_name?: string | null;
  scope_town_name?: string | null;
  cid_short_name?: string | null;
};

export type AdminIdentifyResult = {
  account_id: string;
  institution_cid_number: string;
  institution_code: string;
  admin_level?: string | null;
  capabilities?: CapabilitySet;
  workspace?: InstitutionWorkspace;
  family_name: string;
  given_name: string;
  scope_province_name?: string | null;
  scope_city_name?: string | null;
  scope_town_name?: string | null;
  cid_short_name?: string | null;
};

export type AdminQrSignRequestResult = {
  challenge_id: string;
  challenge_payload: string;
  login_qr_payload: string;
  origin: string;
  domain: string;
  session_id: string;
  expire_at: number;
};

export type AdminVerifyResult = {
  access_token: string;
  expire_at: number;
  admin: AdminIdentifyResult;
};

export type AdminInstitutionCandidate = {
  candidate_id: string;
  institution_code: string;
  admin_level?: string | null;
  institution_cid_number?: string | null;
  frg_province_code?: string | null;
  cid_full_name?: string | null;
  cid_short_name?: string | null;
  scope_province_name?: string | null;
  scope_city_name?: string | null;
  scope_town_name?: string | null;
};

export type NodeBindingRequired = {
  binding_challenge_id: string;
  account_id: string;
  candidates: AdminInstitutionCandidate[];
};

export type AdminLoginCompleteResult = {
  status: 'SUCCESS' | 'BINDING_REQUIRED';
  access_token?: string;
  expire_at?: number;
  admin?: AdminIdentifyResult;
  binding?: NodeBindingRequired;
};

export type AdminQrLoginStatus = {
  status: 'PENDING' | 'SUCCESS' | 'EXPIRED';
  message: string;
  access_token?: string;
  expire_at?: number;
  admin?: AdminIdentifyResult;
};

export async function createAdminQrSignRequest(input: {
  identity_qr: string;
  origin: string;
  session_id: string;
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
  challenge_id: string;
  session_id?: string;
  account_id: string;
  signature: string;
}): Promise<AdminLoginCompleteResult> {
  return request<AdminLoginCompleteResult>('/api/admin/auth/qr/complete', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input),
  });
}

export async function confirmNodeBinding(input: {
  binding_challenge_id: string;
  candidate_id: string;
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
