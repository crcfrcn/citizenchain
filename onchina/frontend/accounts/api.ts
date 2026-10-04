// 机构账户前端 API。
//
// 机构自定义账户的新增/删除都不再本地直写:改为向后端拿「本机构内部投票提案」的裸 call
// (PrepareInstitutionChainOutput),由发起管理员使用签名钱包冷签一笔普通 extrinsic 上链,机构内部
// 投票通过后才生效。冷签扫码 + 提交复用 core/useChainSign。账户列表读侧已切链上真源。

import type { AdminAuth } from '../auth/types';
import type { PrepareInstitutionChainOutput } from '../admins/api';
import { adminRequest } from '../utils/http';
import type { InstitutionAccount } from '../subjects/api';

export type { InstitutionAccount, MultisigChainStatus } from '../subjects/api';

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
      body: JSON.stringify({ account_name: accountName, proposer_role_code: proposerRoleCode }),
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
      body: JSON.stringify({ proposer_role_code: proposerRoleCode }),
    },
  );
}
