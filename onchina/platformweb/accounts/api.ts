// 查询机构账户并请求新增或关闭账户的链上提案准备。

import type { AdminAuth } from '../authentication/types';
import type { PrepareInstitutionChainOutput } from '../institutions/governance';
import { adminRequest } from "../workspace/http";
import type { InstitutionAccount } from '../institutions/api';

export type { InstitutionAccount, MultisigChainStatus } from '../institutions/api';

// 新增机构自定义账户 = 发起本机构「新增账户」内部投票提案(runtime call 7)。
// 返回 sign_request,交由 useChainSign 扫码/钱包签名后提交 /api/admin/chain/submit。
export async function createAccount(
  auth: AdminAuth,
  cidNumber: string,
  accountName: string,
  proposerRoleCode: string,
): Promise<PrepareInstitutionChainOutput> {
  return adminRequest<PrepareInstitutionChainOutput>(
    `/api/institutions/${encodeURIComponent(cidNumber)}/account/create`,
    auth,
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ accountName: accountName, proposerRoleCode: proposerRoleCode }),
    },
  );
}

export async function listAccounts(
  auth: AdminAuth,
  cidNumber: string,
): Promise<InstitutionAccount[]> {
  return adminRequest<InstitutionAccount[]>(
    `/api/institutions/${encodeURIComponent(cidNumber)}/accounts`,
    auth,
  );
}

// 关闭机构自定义账户 = 发起本机构「关闭账户」内部投票提案(runtime call 1)。
// DELETE 带 Json body 传岗位码;同样返回 sign_request 走 useChainSign 冷签提交。
export async function deleteAccount(
  auth: AdminAuth,
  cidNumber: string,
  accountName: string,
  proposerRoleCode: string,
): Promise<PrepareInstitutionChainOutput> {
  return adminRequest<PrepareInstitutionChainOutput>(
    `/api/institutions/${encodeURIComponent(cidNumber)}/account/${encodeURIComponent(accountName)}`,
    auth,
    {
      method: 'DELETE',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ proposerRoleCode: proposerRoleCode }),
    },
  );
}
