// 管理员安全动作 API。
// 管理端权限统一为 SESSION / PASSKEY / PASSKEY_COLD_SIGN 三档。
// PASSKEY_COLD_SIGN 动作走 prepare → CitizenWallet 扫码签名一次并显示响应二维码 → OnChina 回扫 commit。

import type { AdminAuth } from '../auth/types';
import { assertPasskey, PASSKEY_ASSERTION_HEADER } from '../auth/passkey/passkeyClient';
import { ApiError, adminRequest } from '../utils/http';

export const SECURITY_GRANT_HEADER = 'x-cid-security-grant';

export type AdminActionType =
  | 'CREATE_SUBORDINATE_REGISTRY'
  | 'DELETE_SUBORDINATE_REGISTRY'
  | 'INSTITUTION_CREATE'
  | 'INSTITUTION_UPDATE'
  | 'INSTITUTION_CREATE_ACCOUNT'
  | 'INSTITUTION_DELETE_ACCOUNT'
  | 'INSTITUTION_UPLOAD_DOCUMENT'
  | 'INSTITUTION_DELETE_DOCUMENT'
  | 'NODE_BINDING_UNBIND'
  | 'CITIZEN_ONCHAIN_PUSH';

export type AdminOperationAuth = 'SESSION' | 'PASSKEY' | 'PASSKEY_COLD_SIGN';

export type PrepareAdminActionOutput = {
  action_id: string;
  action_type: AdminActionType;
  actor_cid_number: string;
  sign_request?: string | null;
  payload_hash: string;
  auth_type: AdminOperationAuth;
  expires_at: number;
};

export type AdminSecurityGrantOutput = {
  grant_id: string;
  action_type: AdminActionType;
  actor_cid_number: string;
  auth_type: AdminOperationAuth;
  target: string;
  expires_at: number;
};

export function formatAdminCreateError(error: unknown, fallback: string): string {
  if (!(error instanceof ApiError)) {
    return error instanceof Error ? error.message : fallback;
  }
  // 管理员新增失败统一按稳定 error_code 显示,不解析后端 message。
  if (error.errorCode === 'ONCHINA_ACCOUNT_ID_EXISTS_AS_FEDERAL_REGISTRY_ADMIN') {
    return '该账户已是联邦注册局管理员，不能新增为市注册局管理员';
  }
  if (error.errorCode === 'ONCHINA_ACCOUNT_ID_EXISTS_AS_CITY_REGISTRY_ADMIN') {
    return '该账户已是市注册局管理员，不能重复新增';
  }
  if (error.errorCode === 'ONCHINA_ADMIN_CITY_REGISTRY_CITY_LIMIT_REACHED') {
    return '本市市注册局管理员已满 30 人，不能继续新增';
  }
  return error.message || fallback;
}

export async function prepareAdminAction(
  auth: AdminAuth,
  actionType: AdminActionType,
  payload: unknown,
): Promise<PrepareAdminActionOutput> {
  return adminRequest<PrepareAdminActionOutput>('/api/admin/actions/prepare', auth, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ action_type: actionType, payload }),
  });
}

// PASSKEY_COLD_SIGN commit 只携带 CitizenWallet 扫码签名响应字段。
export async function commitAdminAction<T>(
  auth: AdminAuth,
  input: {
    action_id: string;
    account_id: string;
    signature: string;
    payload_hash: string;
  },
): Promise<T> {
  return adminRequest<T>('/api/admin/actions/commit', auth, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input),
  });
}

// 组件提供的「扫码签名」回调:给定已 prepare 的 PASSKEY_COLD_SIGN 动作,
// 弹出公民钱包二维码并扫描签名响应,解析出 account_id/signature 回传。
export type ScanSignResolver = (
  prepared: PrepareAdminActionOutput,
) => Promise<{ account_id: string; signature: string }>;

// 统一的 PASSKEY_COLD_SIGN 安全授权:prepare → 组件扫码签名 → commit 取回一次性 grant。
// SESSION 动作不走这里(无 commit,业务 handler 仅凭会话执行)。
export async function createScanSignSecurityGrant(
  auth: AdminAuth,
  actionType: AdminActionType,
  payload: unknown,
  signWithScan: ScanSignResolver,
): Promise<AdminSecurityGrantOutput> {
  const prepared = await prepareAdminAction(auth, actionType, payload);
  if (prepared.auth_type !== 'PASSKEY_COLD_SIGN' || !prepared.sign_request) {
    throw new Error('该操作缺少公民钱包扫码签名请求');
  }
  const { account_id, signature } = await signWithScan(prepared);
  return commitAdminAction<AdminSecurityGrantOutput>(auth, {
    action_id: prepared.action_id,
    account_id,
    signature,
    payload_hash: prepared.payload_hash,
  });
}

// PASSKEY_COLD_SIGN 正式业务提交必须同时携带两份一次性凭证:
// 1) CitizenWallet 扫码签名一次得到的 security grant;
// 2) 当前管理员本机 passkey 断言。
// 后端 require_admin_security_grant 会先消费 passkey,再消费 grant;二者缺一即 fail-closed。
export async function securityGrantSubmitHeaders(
  auth: AdminAuth,
  securityGrant: AdminSecurityGrantOutput,
  baseHeaders: Record<string, string> = {},
): Promise<Record<string, string>> {
  const passkeyAssertion = await assertPasskey(auth);
  return {
    ...baseHeaders,
    [SECURITY_GRANT_HEADER]: securityGrant.grant_id,
    [PASSKEY_ASSERTION_HEADER]: passkeyAssertion,
  };
}

// 本地写(Passkey 档)提交头:只需当前管理员本机 passkey 断言,不走 prepare/扫码签名/commit。
// 后端 require_admin_security_grant 对 Passkey 档只校验 passkey 断言 + 角色,不消费冷签 grant。
export async function passkeySubmitHeaders(
  auth: AdminAuth,
  baseHeaders: Record<string, string> = {},
): Promise<Record<string, string>> {
  const passkeyAssertion = await assertPasskey(auth);
  return {
    ...baseHeaders,
    [PASSKEY_ASSERTION_HEADER]: passkeyAssertion,
  };
}

// 最常用路径：prepare → CitizenWallet 一次签名响应回扫 commit → passkey → 返回正式业务提交头。
// 调用方只负责传入与业务请求逐字段一致的 payload,避免授权和提交出现第二真源。
export async function createColdSignSubmitHeaders(
  auth: AdminAuth,
  actionType: AdminActionType,
  payload: unknown,
  signWithScan: ScanSignResolver,
  baseHeaders: Record<string, string> = {},
): Promise<Record<string, string>> {
  const grant = await createScanSignSecurityGrant(auth, actionType, payload, signWithScan);
  return securityGrantSubmitHeaders(auth, grant, baseHeaders);
}
