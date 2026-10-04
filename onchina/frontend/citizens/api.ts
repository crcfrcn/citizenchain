// 公民直接录入 + 列表查询 API。
// 注册局管理员提交档案字段,后端自动生成身份 CID、护照号和护照有效期。
// 通用请求能力只从 utils/http.ts 引入,本文件不承接机构或管理员模块接口。

import type { AdminAuth } from '../auth/types';
import { assertPasskey, PASSKEY_ASSERTION_HEADER } from '../auth/passkey/passkeyClient';
import { adminHeaders, adminRequest, request } from '../utils/http';

export type CitizenState = 'NORMAL' | 'REVOKED';
export type CitizenSex = 'MALE' | 'FEMALE';
export type CitizenOnchainIdentityLevel = 'voting' | 'candidate';

function jsonAdminHeaders(auth: AdminAuth): Record<string, string> {
  return {
    'content-type': 'application/json',
    ...Object.fromEntries(new Headers(adminHeaders(auth)).entries()),
  };
}

export type CitizenRow = {
  id: number;
  cid_number: string;
  passport_no: string;
  family_name: string;
  given_name: string;
  citizen_sex: CitizenSex;
  citizen_birth_date: string;
  account_id?: string | null;
  ss58_address?: string | null;
  binding_revision: number;
  binding_finalized_block_number?: number | null;
  binding_finalized_block_hash?: string | null;
  citizen_status: CitizenState;
  voting_eligible: boolean;
  vote_status: CitizenState;
  identity_status: CitizenState;
  passport_valid_from: string;
  passport_valid_until: string;
  status_updated_at?: number;
  province_code: string;
  city_code: string;
  town_code: string;
  province_name?: string;
  city_name?: string;
  town_name?: string;
  birth_province_code: string;
  birth_city_code: string;
  birth_town_code: string;
  birth_province_name?: string;
  birth_city_name?: string;
  birth_town_name?: string;
  archive_hash?: string;
  onchain_tx_hash?: string;
  onchain_block_number?: number;
  onchain_at?: string;
};

export type PageResult<T> = {
  items: T[];
  page_size: number;
  next_cursor?: string | null;
  has_more: boolean;
};

/** 人主体类型:CTZN(公民)/ NATP(居民)。 */
export type CitizenType = 'CTZN' | 'NATP';

/**
 * 占号 prepare 请求 DTO。占即绑:仅岗位码 + 人主体类型必填,居住省市由办理注册局 scope 派生,
 * 姓名/性别/出生/护照等档案占号后为空、后续经「编辑资料」补齐(与后端 AdminCreateCitizenInput 对齐)。
 */
export type CreateCitizenInput = {
  actor_role_code: string;
  cid_type: CitizenType;
};

/** 直接录入公民返回 DTO。 */
export type CreateCitizenResult = {
  id: number;
  cid_number: string;
  passport_no: string;
  family_name: string;
  given_name: string;
  citizen_sex: CitizenSex;
  citizen_birth_date: string;
  citizen_status: CitizenState;
  voting_eligible: boolean;
  account_id?: string | null;
  ss58_address?: string | null;
  passport_valid_from: string;
  passport_valid_until: string;
  province_code: string;
  city_code: string;
  town_code: string;
  birth_province_code: string;
  birth_city_code: string;
  birth_town_code: string;
  archive_hash?: string;
};

export type PrepareCitizenOnchainResult = {
  cid_number: string;
  actor_role_code: string;
  identity_level: CitizenOnchainIdentityLevel;
  account_id: string;
  ss58_address: string;
  payload_hex: string;
  sign_request: string;
  action_label_zh: string;
  expires_at: number;
};

export type CompleteCitizenOnchainResult = {
  request_id: string;
  cid_number: string;
  actor_role_code: string;
  identity_level: CitizenOnchainIdentityLevel;
  account_id: string;
  ss58_address: string;
  chain_action: number;
  call_data_hex: string;
  citizen_signature: string;
  citizen_identity_chain_sign_request: string;
};

export const CITIZEN_DOCUMENT_TYPES = ['护照相片', '出生证明', '监护人护照', '其他材料'] as const;

export type CitizenDocumentType = (typeof CITIZEN_DOCUMENT_TYPES)[number];

export type CitizenDocument = {
  id: number;
  cid_number: string;
  file_name: string;
  document_type: CitizenDocumentType;
  file_size: number;
  file_hash: string;
  uploader_account_id: string;
  uploaded_at: string;
};

export interface LegalRepresentativeCitizenSearchContext {
  target_cid_number?: string;
  province_name?: string;
  city_name?: string;
  subject_property?: string;
  institution?: string;
  education_type?: string;
  parent_cid_number?: string;
}

export async function listCitizens(
  auth: AdminAuth,
  keyword: string,
  provinceName: string,
  cityName: string,
  cursor?: string | null,
  pageSize = 50,
): Promise<PageResult<CitizenRow>> {
  const params = new URLSearchParams({
    keyword,
    province_name: provinceName,
    city_name: cityName,
    page_size: String(pageSize),
  });
  if (cursor) params.set('cursor', cursor);
  return request<PageResult<CitizenRow>>(`/api/admin/citizens?${params.toString()}`, {
    headers: adminHeaders(auth),
  });
}

/**
 * 编辑公民资料输入。
 *
 * 可变:姓、名、居住市、居住镇、选举资格。不可变(现实不可变,初始化后锁定):
 * 性别/出生日期/出生地;护照号与有效期由服务端在出生日期就绪时确定性签发,不在此提交。
 * 居住省(= CID 省)与身份 CID 不变;跨省居住迁移属"跨地区",后续单独处理。
 */
export type EditCitizenInput = {
  family_name: string;
  given_name: string;
  citizen_sex: CitizenSex | '';
  citizen_birth_date: string;
  birth_province_code: string;
  birth_city_code: string;
  birth_town_code: string;
  city_code: string;
  town_code: string;
  voting_eligible: boolean;
};

/** 编辑/补齐公民本地档案(不可变字段锁定),返回最新档案行。 */
export async function editCitizen(
  auth: AdminAuth,
  cidNumber: string,
  payload: EditCitizenInput,
): Promise<CitizenRow> {
  return request<CitizenRow>(
    `/api/admin/citizens/${encodeURIComponent(cidNumber)}/edit`,
    {
      method: 'POST',
      headers: jsonAdminHeaders(auth),
      body: JSON.stringify(payload),
    },
  );
}

export async function searchLegalRepresentativeCitizens(
  auth: AdminAuth,
  q: string,
  context: LegalRepresentativeCitizenSearchContext,
  pageSize = 20,
): Promise<string[]> {
  const params = new URLSearchParams({
    q: q.trim(),
    page_size: String(pageSize),
  });
  Object.entries(context).forEach(([key, value]) => {
    const trimmed = typeof value === 'string' ? value.trim() : '';
    if (trimmed) params.set(key, trimmed);
  });
  return request<string[]>(`/api/admin/citizens/legal-representatives?${params.toString()}`, {
    headers: adminHeaders(auth),
  });
}

/**
 * 占号 prepare 返回(段1):发号 + **公民钱包域签名 QR**(占即绑,b.u 空、钱包自填本账户)。
 * 此步不落任何档案 —— 公民占号签名回来 → 段2 组装 occupy_cid →
 * 管理员冷签交易 finalized+ExtrinsicSuccess 且同块状态核验通过后才建档。
 */
export type PrepareCitizenOccupyResult = {
  request_id: string;
  cid_number: string;
  citizen_sign_request: string;
  expires_at: number;
};

/** 占号 submit(段2)返回:管理员冷签 QR(段3 chain/submit 用)。 */
export type SubmitCitizenOccupyResult = {
  request_id: string;
  cid_number: string;
  sign_request: string;
  expires_at: number;
};

/** 换绑 prepare(段1)返回:**新钱包域签名 QR**(b.u 空、完整授权模板含零账户槽)。 */
export type PrepareCitizenRebindResult = {
  request_id: string;
  cid_number: string;
  new_wallet_sign_request: string;
  expires_at: number;
};

/** 换绑 submit(段2)返回:管理员冷签 QR。 */
export type SubmitCitizenRebindResult = {
  request_id: string;
  cid_number: string;
  sign_request: string;
  expires_at: number;
};

/** 所有注册局可见的 finalized CID 公开绑定，不包含任何链下公民档案。 */
export type FinalizedCitizenBinding = {
  cid_number: string;
  cid_status: 'ACTIVE' | 'REVOKED';
  binding_active: boolean;
  account_id?: string | null;
  ss58_address?: string | null;
  binding_revision?: number | null;
  voting_identity: boolean;
  candidate_identity: boolean;
  registry_rebind_required: boolean;
  registrar_cid_number?: string | null;
  finalized_block_number: number;
  finalized_block_hash: string;
  genesis_hash: string;
};

/** 吊销 prepare 返回:冷签 QR。 */
export type PrepareCitizenRevokeResult = {
  request_id: string;
  cid_number: string;
  sign_request: string;
  expires_at: number;
};

/**
 * 占号段1 prepare:后端发号,返回给公民本人钱包/公民App 的域签名占号 QR(占即绑)。
 * 此步不落任何档案 —— 公民签名回来后走段2、段3 进块才建匿名记录。
 */
export async function prepareCitizenOccupy(
  auth: AdminAuth,
  payload: CreateCitizenInput,
): Promise<PrepareCitizenOccupyResult> {
  // 链上写(occupy_cid)= passkey + 冷签:prepare 段消费一次 passkey 断言(冷签在段3 chain/submit)。
  const assertion = await assertPasskey(auth);
  const headers = { ...jsonAdminHeaders(auth), [PASSKEY_ASSERTION_HEADER]: assertion };
  return request<PrepareCitizenOccupyResult>('/api/admin/citizens', {
    method: 'POST',
    headers,
    body: JSON.stringify(payload),
  });
}

/**
 * 占号段2 submit:回传公民钱包的占号签名(account_id + occupy_signature),
 * 后端验签 → 组装 occupy_cid 占即绑交易 → 返回管理员冷签 QR(段3 用)。
 */
export async function submitCitizenOccupy(
  auth: AdminAuth,
  requestId: string,
  account_id: string,
  occupy_signature: string,
): Promise<SubmitCitizenOccupyResult> {
  return request<SubmitCitizenOccupyResult>('/api/admin/citizens/occupy/submit', {
    method: 'POST',
    headers: jsonAdminHeaders(auth),
    body: JSON.stringify({ request_id: requestId, account_id, occupy_signature }),
  });
}

/**
 * 换绑段1 prepare:直接按链上 finalized 绑定真源发起。
 * 匿名 CID 可由任一在册 CREG/FRG 办理；实名 CID 仅本市 CREG/对应省 FRG，
 * 最终辖区权限由 Runtime 统一裁决。新钱包签名，不要求当前钱包。
 */
export async function prepareCitizenRebind(
  auth: AdminAuth,
  cidNumber: string,
  actorRoleCode: string,
): Promise<PrepareCitizenRebindResult> {
  // 链上写(admin_rebind_cid_account_id)= passkey + 冷签:prepare 段消费一次 passkey 断言(冷签在段3)。
  const assertion = await assertPasskey(auth);
  const headers = { ...jsonAdminHeaders(auth), [PASSKEY_ASSERTION_HEADER]: assertion };
  return request<PrepareCitizenRebindResult>('/api/admin/citizens/rebind/prepare', {
    method: 'POST',
    headers,
    body: JSON.stringify({ actor_role_code: actorRoleCode, cid_number: cidNumber }),
  });
}

/**
 * 换绑段2 submit:回传新钱包的换绑签名(new_account_id + new_account_signature),
 * 后端验签 → 组装 admin_rebind_cid_account_id → 返回管理员冷签 QR(段3 用)。
 */
export async function submitCitizenRebind(
  auth: AdminAuth,
  requestId: string,
  newAccountId: string,
  newAccountSignature: string,
  currentAccountId?: string,
  currentAccountSignature?: string,
): Promise<SubmitCitizenRebindResult> {
  return request<SubmitCitizenRebindResult>('/api/admin/citizens/rebind/submit', {
    method: 'POST',
    headers: jsonAdminHeaders(auth),
    body: JSON.stringify({
      request_id: requestId,
      new_account_id: newAccountId,
      new_account_signature: newAccountSignature,
      ...(currentAccountId ? { current_account_id: currentAccountId } : {}),
      ...(currentAccountSignature
        ? { current_account_signature: currentAccountSignature }
        : {}),
    }),
  });
}

export async function fetchFinalizedCitizenBinding(
  auth: AdminAuth,
  cidNumber: string,
): Promise<FinalizedCitizenBinding> {
  return request<FinalizedCitizenBinding>(
    `/api/admin/citizens/${encodeURIComponent(cidNumber)}/binding`,
    { headers: adminHeaders(auth) },
  );
}

/**
 * 吊销 prepare:登记表墓碑(号永不复用),最严档 PASSKEY_COLD_SIGN grant。
 * 返回冷签 QR,回签后同样走 core 统一提交入口。
 */
export async function prepareCitizenRevoke(
  auth: AdminAuth,
  cidNumber: string,
  actorRoleCode: string,
): Promise<PrepareCitizenRevokeResult> {
  const assertion = await assertPasskey(auth);
  const headers = { ...jsonAdminHeaders(auth), [PASSKEY_ASSERTION_HEADER]: assertion };
  return request<PrepareCitizenRevokeResult>(
    `/api/admin/citizens/${encodeURIComponent(cidNumber)}/onchain/revoke/prepare`,
    {
      method: 'POST',
      headers,
      body: JSON.stringify({ actor_role_code: actorRoleCode }),
    },
  );
}

export async function prepareCitizenOnchainSignature(
  auth: AdminAuth,
  cidNumber: string,
  account_id: string,
  actorRoleCode: string,
  identityLevel: CitizenOnchainIdentityLevel,
): Promise<PrepareCitizenOnchainResult> {
  // 一次业务操作只在创建时消费一次 Passkey；后续回执由短期操作会话关联。
  const assertion = await assertPasskey(auth);
  const headers = { ...jsonAdminHeaders(auth), [PASSKEY_ASSERTION_HEADER]: assertion };
  return request<PrepareCitizenOnchainResult>(
    `/api/admin/citizens/${encodeURIComponent(cidNumber)}/onchain/prepare`,
    {
      method: 'POST',
      headers,
      body: JSON.stringify({
        account_id,
        actor_role_code: actorRoleCode,
        identity_level: identityLevel,
      }),
    },
  );
}

export async function completeCitizenOnchainSignature(
  auth: AdminAuth,
  cidNumber: string,
  account_id: string,
  actorRoleCode: string,
  identityLevel: CitizenOnchainIdentityLevel,
  signResponse: string,
): Promise<CompleteCitizenOnchainResult> {
  const headers = jsonAdminHeaders(auth);
  return request<CompleteCitizenOnchainResult>(
    `/api/admin/citizens/${encodeURIComponent(cidNumber)}/onchain/complete`,
    {
      method: 'POST',
      headers,
      body: JSON.stringify({
        account_id,
        actor_role_code: actorRoleCode,
        identity_level: identityLevel,
        sign_response: signResponse,
      }),
    },
  );
}

export async function listCitizenDocuments(
  auth: AdminAuth,
  cidNumber: string,
): Promise<CitizenDocument[]> {
  return adminRequest<CitizenDocument[]>(
    `/api/admin/citizens/${encodeURIComponent(cidNumber)}/documents`,
    auth,
  );
}

// 公民资料库独立于机构资料库,字段名使用 document_type,不复用机构 doc_type。
export async function uploadCitizenDocument(
  auth: AdminAuth,
  cidNumber: string,
  file: File,
  documentType: CitizenDocumentType,
): Promise<CitizenDocument> {
  const formData = new FormData();
  formData.append('file', file);
  formData.append('document_type', documentType);
  return adminRequest<CitizenDocument>(
    `/api/admin/citizens/${encodeURIComponent(cidNumber)}/documents`,
    auth,
    {
      method: 'POST',
      body: formData,
    },
  );
}

export async function downloadCitizenDocument(
  auth: AdminAuth,
  cidNumber: string,
  docId: number,
  fileName: string,
): Promise<void> {
  const resp = await fetch(
    `/api/admin/citizens/${encodeURIComponent(cidNumber)}/documents/${docId}/download`,
    { headers: adminHeaders(auth) },
  );
  if (!resp.ok) throw new Error(`下载失败 (${resp.status})`);
  const blob = await resp.blob();
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = fileName;
  a.click();
  URL.revokeObjectURL(url);
}

export async function deleteCitizenDocument(
  auth: AdminAuth,
  cidNumber: string,
  docId: number,
): Promise<void> {
  await adminRequest<string>(
    `/api/admin/citizens/${encodeURIComponent(cidNumber)}/documents/${docId}`,
    auth,
    { method: 'DELETE' },
  );
}
