// 读取注册局链上资料，准备正常岗位配置并关联原签名回执。

import { type AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { adminRequest } from '@gmb/onchina-platform/http';
import { passkeySubmitHeaders } from '@gmb/onchina-platform/authentication/security';
import { getGovernanceState, governanceProposers, institutionRegistrationMatches, registrationActorRole, prepareInstitutionGovernance, type GovernanceState, type InstitutionRegistrationInput, type PrepareInstitutionChainOutput } from '@gmb/onchina-platform/institutions/governance';
import { submitChainSign, type ChainSignResult } from '@gmb/onchina-platform/qr/useChainSign';
import { ApiError } from '@gmb/onchina-platform/http';

/** 岗位入口拒绝退休的钱包载荷，包含已签回执；拒绝发生在认证和广播之前。 */
function ensureRoleInput(input: InstitutionRegistrationInput): void {
  const personRows = [...(input?.assignmentChanges ?? []).flatMap(c => c.assignments), ...(input?.roleMutations ?? []).flatMap(m => m.assignments ?? [])];
  if (personRows.some(a => !a.personCid || 'accountId' in a) || input?.roleMutations?.some(m => m.mutation === 'CREATE' && (!Number.isInteger(m.seats) || m.seats! < 1))) throw new Error('岗位配置只接受人员CID任职和明确席位');
  const fields = ['cidNumber', 'expectedStateHash', 'actorRoleCode', 'initialThreshold', 'roleMutations', 'assignmentChanges'];
  if (!input || typeof input !== 'object' || Object.keys(input).some(key => !fields.includes(key)) ||
      !(input.roleMutations?.length || input.assignmentChanges?.length)) throw new Error('岗位配置不接收管理员钱包或名单变更');
}

/** 正常岗位配置沿用一次Passkey认证和原链签名提交流程。 */
export async function prepareRegisterInstitutionRoles(
  auth: AdminAuth,
  input: InstitutionRegistrationInput,
  signal?: AbortSignal,
): Promise<PrepareInstitutionChainOutput> {
  ensureRoleInput(input);
  if (!/^0x[0-9a-f]{64}$/.test(input.expectedStateHash ?? '')) throw new Error('请刷新并核对机构岗位配置状态');
  const headers = await passkeySubmitHeaders(auth, { 'content-type': 'application/json' });
  signal?.throwIfAborted();
  return adminRequest<PrepareInstitutionChainOutput>('/api/admin/institution/roles/register/prepare', auth, {
    method: 'POST', headers, signal,
    body: JSON.stringify(input),
  });
}

/** 只保存已签名会话的恢复所需字段；重试使用同一请求和签名，不重新prepare。 */
export type InstitutionRegistrationMode = 'registration' | 'governance';
export type PendingInstitutionRegistration = { mode: InstitutionRegistrationMode; personCid:string; bindingRevision:string; roleCode:string; actorInstitutionCid:string; requestId: string; cidNumber: string; accountId: string; signature: string; expected: InstitutionRegistrationInput; reviewedBlockNumber: number };
function receiptKey(auth: AdminAuth, cid: string, mode: InstitutionRegistrationMode): string {
  return `institution-registration:${auth.personCid}:${auth.institutionCidNumber}:${auth.roleCode}:${cid}:${mode}`;
}
export function readRegistrationReceipt(auth: AdminAuth, cid: string, mode: InstitutionRegistrationMode = 'registration'): PendingInstitutionRegistration | null {
  try {
    const value = JSON.parse(sessionStorage.getItem(receiptKey(auth, cid, mode)) ?? 'null') as PendingInstitutionRegistration | null;
    if (value) ensureRoleInput(value.expected);
    if (!value || value.mode !== mode || value.cidNumber !== cid || value.expected?.cidNumber !== cid || !Number.isSafeInteger(value.reviewedBlockNumber) || value.personCid !== auth.personCid || value.roleCode !== auth.roleCode || value.actorInstitutionCid !== auth.institutionCidNumber || !value.bindingRevision ||
      !/^institution-governance-[a-zA-Z0-9-]+$/.test(value.requestId) || !/^0x[0-9a-fA-F]{128}$/.test(value.signature)) return null;
    return value;
  } catch { return null; }
}
export function saveRegistrationReceipt(auth: AdminAuth, cid: string, value: PendingInstitutionRegistration | null, mode: InstitutionRegistrationMode = 'registration'): void {
  if (value) ensureRoleInput(value.expected);
  // 缓存失败不改变链签会话和内存中的原签名；服务端仍保留可恢复的提交记录。
  try { if (value) sessionStorage.setItem(receiptKey(auth, cid, mode), JSON.stringify(value)); else sessionStorage.removeItem(receiptKey(auth, cid, mode)); } catch { /* 浏览器可禁用存储。 */ }
}


/** 统一机构登记提交和恢复；UI只负责收集明确变更及展示钱包二维码。 */
export async function runInstitutionRegistration({ auth, cidNumber, input, pending, sign, remember, check, signal, mode = 'registration' }: {
  auth: AdminAuth; cidNumber: string; input: InstitutionRegistrationInput | null;
  pending: PendingInstitutionRegistration | null;
  sign: (requestId: string, request: string) => Promise<ChainSignResult>;
  remember: (receipt: PendingInstitutionRegistration | null) => void;
  check: () => void; signal?: AbortSignal; mode?: InstitutionRegistrationMode;
}): Promise<{ recovered: boolean; state: GovernanceState }> {
  check();
  if (input) ensureRoleInput(input);
  if (pending) ensureRoleInput(pending.expected);
  let receipt = pending;
  if (!receipt) {
    if (!input || input.cidNumber !== cidNumber) throw new Error('登记目标与当前页面不一致');
    const fresh = await getGovernanceState(auth, cidNumber); check();
    if (fresh.cidNumber !== cidNumber || fresh.registrationStateHash !== input.expectedStateHash) throw new Error('机构状态已变化，请刷新后重新核对签名');
    registrationActorRole(mode === 'governance' ? { ...fresh, registrationRoles: governanceProposers(auth, fresh).map(role => ({ roleCode: role.roleCode, roleName: role.roleName })) } : fresh, input.actorRoleCode);
    if (mode === 'governance' && auth.institutionCidNumber !== cidNumber) throw new Error('只能发起本机构治理');
    const prepared = mode === 'registration' ? await prepareRegisterInstitutionRoles(auth, input, signal) : await prepareInstitutionGovernance(auth, {
      cidNumber, proposerRoleCode: input.actorRoleCode!,
      assignmentChanges: input.assignmentChanges,
    }, signal); check();
    if (prepared.cidNumber !== cidNumber || !Number.isFinite(prepared.expiresAt) || !prepared.requestId || !prepared.signRequest || prepared.expiresAt * 1000 <= Date.now()) throw new Error('签名请求目标不匹配或已过期');
    const signed = await sign(prepared.requestId, prepared.signRequest); check();
    if (signed.accountId.toLowerCase() !== auth.accountId.toLowerCase() || !/^0x[0-9a-fA-F]{128}$/.test(signed.signature)) throw new Error('签名钱包或签名格式与当前管理员不一致');
    receipt = { mode, personCid:auth.personCid,bindingRevision:auth.bindingRevision,roleCode:auth.roleCode,actorInstitutionCid:auth.institutionCidNumber, requestId: prepared.requestId, cidNumber, accountId: signed.accountId, signature: signed.signature,
      expected: structuredClone(input), reviewedBlockNumber: fresh.blockNumber };
    remember(receipt);
  }
  if (receipt.mode !== mode || receipt.cidNumber !== cidNumber || receipt.expected.cidNumber !== cidNumber || receipt.personCid !== auth.personCid || receipt.roleCode !== auth.roleCode || receipt.actorInstitutionCid !== auth.institutionCidNumber) throw new Error('原签名会话不属于当前身份或目标');
  check();
  let result;
  try { result = await submitChainSign(auth, receipt.requestId, receipt.accountId, receipt.signature); }
  catch (error) {
    check();
    // 已完成会话可能已消费/清理。只有链上目标结果完全吻合才恢复为成功。
    if (error instanceof ApiError && [404, 409].includes(error.status)) {
      const settled = await getGovernanceState(auth, cidNumber); check();
      if (settled.blockNumber > receipt.reviewedBlockNumber && institutionRegistrationMatches(settled, receipt.expected)) {
        remember(null); return { recovered: true, state: settled };
      }
    }
    if (error instanceof ApiError && [400, 403, 410, 422].includes(error.status)) remember(null);
    throw error;
  }
  check();
  // prepare已统一目标键；返回的目标必须与冻结载荷一致，空字段也拒绝。
  if (result.purpose !== (mode === 'governance' ? 'INSTITUTION_GOVERNANCE' : 'INSTITUTION_REGISTER_ROLES') || result.cidNumber !== cidNumber || !result.txHash || !Number.isSafeInteger(result.blockNumber) || result.blockNumber! < 0) throw new Error('提交结果尚未确认，请恢复原会话');
  const settled = await getGovernanceState(auth, cidNumber); check();
  if (settled.blockNumber < result.blockNumber! || settled.blockNumber <= receipt.reviewedBlockNumber || (mode === 'registration' && !institutionRegistrationMatches(settled, receipt.expected))) throw new Error('链上登记结果尚未核对，请恢复原会话');
  remember(null); return { recovered: false, state: settled };
}
