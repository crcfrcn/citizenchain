// 读取注册局链上名册，准备机构管理员登记并关联提交回执。

import { type AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { adminHeaders, adminRequest, request } from '@gmb/onchina-platform/http';
import { type CidCityItem } from '@gmb/onchina-platform/codes/api';
import { passkeySubmitHeaders } from '@gmb/onchina-platform/authentication/security';
import { type InstitutionDetail } from '@gmb/onchina-platform/institutions/api';
import { getGovernanceState, governanceProposers, institutionRegistrationMatches, registrationActorRole, prepareInstitutionGovernance, type GovernanceState, type InstitutionRegistrationInput, type PrepareInstitutionChainOutput } from '@gmb/onchina-platform/institutions/governance';
import { submitChainSign, type ChainSignResult } from '@gmb/onchina-platform/qr/useChainSign';
import { ApiError } from '@gmb/onchina-platform/http';
// 市注册局管理员 API。
// 市注册局管理员列表、创建、删除都归入 admins 管理员功能目录。

export type CityRegistryAdminRow = {
  id: string;
  institutionCidNumber: string;
  rosterAccountId: string;
  citizenCidNumber: string;
  roleName: string;
  accountId: string;
  familyName: string;
  givenName: string;
  balanceFen?: string | null;
  institutionCode: string;
  creatorAccountId: string;
  creatorFamilyName: string;
  creatorGivenName: string;
  createdAt: string | null;
  cityName: string;
};

/** 当前详情页CID是唯一目标；不做省域全量成员扫描或使用本地目录缓存。 */
export async function listCityRegistryAdmins(auth: AdminAuth, cidNumber: string): Promise<CityRegistryAdminRow[]> {
  const data = await request<{ cidNumber: string; total: number; rows: CityRegistryAdminRow[] }>(`/api/admin/city-registry-admins?cidNumber=${encodeURIComponent(cidNumber)}&limit=1989`, {
    method: 'GET', headers: adminHeaders(auth),
  });
  if (data.cidNumber !== cidNumber || !Array.isArray(data.rows) || data.rows.some(row => row.institutionCidNumber !== cidNumber)) throw new Error('链上名册目标不匹配');
  return data.rows;
}

export { MAX_INSTITUTION_ADMINS } from '@gmb/onchina-platform/institutions/governance';


/** 所有子视图共享的状态与回调 */
export interface RegistryAdminsSharedState {
  federalRegistryAdmins: FederalRegistryAdminRow[];
  federalRegistryAdminsLoading: boolean;
  refreshFederalRegistryAdmins: () => Promise<FederalRegistryAdminRow[]>;
  selectedFederalRegistry: FederalRegistryAdminRow | null;
  setSelectedFederalRegistry: (v: FederalRegistryAdminRow | null) => void;
  /** 当前选中的市(三层导航:省→市→市注册局管理员列表) */
  selectedCity: string | null;
  setSelectedCity: (v: string | null) => void;
  adminDetailTab: 'city-registry-admin' | 'federal-registry-admin';
  setAdminDetailTab: (v: 'city-registry-admin' | 'federal-registry-admin') => void;

  cityRegistryAdmins: CityRegistryAdminRow[];
  cityRegistryAdminsLoading: boolean;
  cityRegistryAdminListPage: number;
  setCityRegistryListPage: (v: number) => void;

  cityRegistryAdminCities: CidCityItem[];
  cityRegistryAdminCitiesLoading: boolean;

  addCityRegistryOpen: boolean;
  setAddCityRegistryOpen: (v: boolean) => void;
  deleteCityRegistryAdmin: CityRegistryAdminRow | null;
  setDeleteCityRegistryAdmin: (row: CityRegistryAdminRow | null) => void;
  refreshCityRegistryAdmins: () => Promise<CityRegistryAdminRow[]>;
  cityRegistryAdminsError: string | null;
  onDeleteCityRegistry: (row: CityRegistryAdminRow) => void;

  /** 联邦注册局机构详情(全国唯一,scope-bypass 接口加载)。用于「联邦注册局」子 tab 详情页。 */
  federalRegistryDetail: InstitutionDetail | null;
  federalRegistryLoading: boolean;
  /** 当前活动市(selectedCity / 市注册局管理员 lockedCityName)对应的市注册局机构 cid_number。 */
  cityRegistryCid: string | null;
  cityRegistryLoading: boolean;
  cityRegistryError: string | null;
  reloadCityRegistry: () => void;
}

export type FederalRegistryAdminRow = {
  id: number;
  provinceName: string;
  accountId: string;
  familyName: string;
  givenName: string;
  roleCode: string;
  roleName: string;
  termRequired: boolean;
  termStart: number;
  termEnd: number;
  assignmentSource: number;
  assignmentSourceLabel: string;
  assignmentSourceRef: string;
  balanceFen?: string | null;
  builtIn: boolean;
  createdAt: string;
  /** 最近一次更新时间,null 表示从未更新。 */
  updatedAt?: string | null;
};

// 所有页面均携带已审阅状态指纹；不再保留缺少指纹的旧类型兼容。
export type PrepareRegisterInstitutionAdminsInput = InstitutionRegistrationInput;

export async function listFederalRegistryAdmins(auth: AdminAuth): Promise<FederalRegistryAdminRow[]> {
  return request<FederalRegistryAdminRow[]>('/api/admin/federal-registry-admins', {
    method: 'GET',
    headers: adminHeaders(auth),
  });
}

/** 登记管理员复用一次Passkey认证，原登记字段与钱包签名流程保持。 */
export async function prepareRegisterInstitutionAdmins(
  auth: AdminAuth,
  input: PrepareRegisterInstitutionAdminsInput,
  signal?: AbortSignal,
): Promise<PrepareInstitutionChainOutput> {
  if (!input.walletChanges && 'admins' in input) throw new Error('岗位配置不接收管理员名单');
  if (!/^0x[0-9a-f]{64}$/.test(input.expectedStateHash ?? '')) throw new Error('请使用管理员列表中的链上登记入口，刷新并核对机构状态');
  const headers = await passkeySubmitHeaders(auth, { 'content-type': 'application/json' });
  signal?.throwIfAborted();
  return adminRequest<PrepareInstitutionChainOutput>('/api/admin/institution/admins/register/prepare', auth, {
    method: 'POST', headers, signal,
    body: JSON.stringify(input),
  });
}

/** 只保存已签名会话的恢复所需字段；重试使用同一请求和签名，不重新prepare。 */
export type InstitutionAdminMode = 'registration' | 'governance';
export type PendingInstitutionRegistration = { mode?: InstitutionAdminMode; requestId: string; cidNumber: string; accountId: string; signature: string; expected: InstitutionRegistrationInput; reviewedBlockNumber: number };
function receiptKey(auth: AdminAuth, cid: string, mode: InstitutionAdminMode): string {
  return `institution-registration:${auth.institutionCidNumber}:${auth.accountId}:${cid}${mode === 'governance' ? ':governance' : ''}`;
}
export function readRegistrationReceipt(auth: AdminAuth, cid: string, mode: InstitutionAdminMode = 'registration'): PendingInstitutionRegistration | null {
  try {
    const value = JSON.parse(sessionStorage.getItem(receiptKey(auth, cid, mode)) ?? 'null') as PendingInstitutionRegistration | null;
    if (!value || (value.mode ?? 'registration') !== mode || value.cidNumber !== cid || value.expected?.cidNumber !== cid || (value.expected.walletChanges ? !Array.isArray(value.expected.admins) : value.expected.admins !== undefined && !Array.isArray(value.expected.admins)) || !Number.isSafeInteger(value.reviewedBlockNumber) || value.accountId.toLowerCase() !== auth.accountId.toLowerCase() ||
      !/^institution-governance-[a-zA-Z0-9-]+$/.test(value.requestId) || !/^0x[0-9a-fA-F]{128}$/.test(value.signature)) return null;
    return value;
  } catch { return null; }
}
export function saveRegistrationReceipt(auth: AdminAuth, cid: string, value: PendingInstitutionRegistration | null, mode: InstitutionAdminMode = 'registration'): void {
  // 缓存失败不改变链签会话和内存中的原签名；服务端仍保留可恢复的提交记录。
  try { if (value) sessionStorage.setItem(receiptKey(auth, cid, mode), JSON.stringify(value)); else sessionStorage.removeItem(receiptKey(auth, cid, mode)); } catch { /* 浏览器可禁用存储。 */ }
}


/** 统一机构登记提交和恢复；UI只负责收集明确变更及展示钱包二维码。 */
export async function runInstitutionRegistration({ auth, cidNumber, input, pending, sign, remember, check, signal, mode = 'registration' }: {
  auth: AdminAuth; cidNumber: string; input: InstitutionRegistrationInput | null;
  pending: PendingInstitutionRegistration | null;
  sign: (requestId: string, request: string) => Promise<ChainSignResult>;
  remember: (receipt: PendingInstitutionRegistration | null) => void;
  check: () => void; signal?: AbortSignal; mode?: InstitutionAdminMode;
}): Promise<{ recovered: boolean; state: GovernanceState }> {
  check();
  let receipt = pending;
  if (!receipt) {
    if (!input || input.cidNumber !== cidNumber) throw new Error('登记目标与当前页面不一致');
    if (mode==='governance' && ('admins' in input || input.walletChanges)) throw new Error('完整名单治理已停用，不自动转换或重签');
    const fresh = await getGovernanceState(auth, cidNumber); check();
    if (fresh.cidNumber !== cidNumber || fresh.registrationStateHash !== input.expectedStateHash) throw new Error('机构状态已变化，请刷新后重新核对签名');
    registrationActorRole(mode === 'governance' ? { ...fresh, registrationRoles: governanceProposers(auth, fresh).map(role => ({ roleCode: role.roleCode, roleName: role.roleName })) } : fresh, input.actorRoleCode);
    if (mode === 'governance' && auth.institutionCidNumber !== cidNumber) throw new Error('只能发起本机构治理');
    const prepared = mode === 'registration' ? await prepareRegisterInstitutionAdmins(auth, input, signal) : await prepareInstitutionGovernance(auth, {
      cidNumber, proposerRoleCode: input.actorRoleCode!,
      assignmentChanges: input.assignmentChanges,
    }, signal); check();
    if (prepared.cidNumber !== cidNumber || !Number.isFinite(prepared.expiresAt) || !prepared.requestId || !prepared.signRequest || prepared.expiresAt * 1000 <= Date.now()) throw new Error('签名请求目标不匹配或已过期');
    const signed = await sign(prepared.requestId, prepared.signRequest); check();
    if (signed.accountId.toLowerCase() !== auth.accountId.toLowerCase() || !/^0x[0-9a-fA-F]{128}$/.test(signed.signature)) throw new Error('签名钱包或签名格式与当前管理员不一致');
    receipt = { mode, requestId: prepared.requestId, cidNumber, accountId: signed.accountId, signature: signed.signature,
      expected: structuredClone(input), reviewedBlockNumber: fresh.blockNumber };
    remember(receipt);
  }
  if ((receipt.mode ?? 'registration') !== mode || receipt.cidNumber !== cidNumber || receipt.expected.cidNumber !== cidNumber || receipt.accountId.toLowerCase() !== auth.accountId.toLowerCase()) throw new Error('原签名会话不属于当前身份或目标');
  check();
  let result;
  try { result = await submitChainSign(auth, receipt.requestId, receipt.accountId, receipt.signature); }
  catch (error) {
    check();
    if (receipt.expected.admins && !receipt.expected.walletChanges) throw error; // 旧0/2只允许核对原回执，不按目标状态推断成功。
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
  if (result.purpose !== (mode === 'governance' ? 'INSTITUTION_GOVERNANCE' : 'INSTITUTION_REGISTER_ADMINS') || result.cidNumber !== cidNumber || !result.txHash || !Number.isSafeInteger(result.blockNumber) || result.blockNumber! < 0) throw new Error('提交结果尚未确认，请恢复原会话');
  const settled = await getGovernanceState(auth, cidNumber); check();
  if (settled.blockNumber < result.blockNumber! || settled.blockNumber <= receipt.reviewedBlockNumber || (mode === 'registration' && !institutionRegistrationMatches(settled, receipt.expected))) throw new Error('链上登记结果尚未核对，请恢复原会话');
  remember(null); return { recovered: false, state: settled };
}
