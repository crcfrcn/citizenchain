// 私权机构前端共用 API。六类私权机构必须在各自目录的 api.ts 中传入 routeSegment,
// 本文件只负责封装共用 HTTP、查重、证件照、详情和参与主体共享接口。

import type { AdminAuth } from '../../auth/types';
import {
  securityGrantSubmitHeaders,
  type AdminSecurityGrantOutput,
} from '../../admins/securityApi';
import { adminRequest } from '../../utils/http';
import type {
  CreateInstitutionInput,
  CreateInstitutionOutput,
  InstitutionCategory,
  InstitutionDetail,
  Institution,
  InstitutionListRow,
  LegalRepresentativePhoto,
  ListInstitutionsQuery,
  PageResult,
  ParentInstitutionRow,
  SearchParentsOptions,
  UpdateInstitutionInput,
} from '../../subjects/api';
import { buildInstitutionCreatePayload } from '../../subjects/api';

export type {
  CreateInstitutionInput,
  CreateInstitutionOutput,
  Institution,
  InstitutionCategory,
  InstitutionDetail,
  InstitutionListRow,
  LegalRepresentativePhoto,
  PageResult,
  ParentInstitutionRow,
  UpdateInstitutionInput,
} from '../../subjects/api';

export async function checkCidFullName(
  auth: AdminAuth,
  cidFullName: string,
  subject_property?: string,
  cityName?: string,
): Promise<{ exists: boolean }> {
  const params = new URLSearchParams({ cid_full_name: cidFullName });
  if (subject_property) params.set('subject_property', subject_property);
  if (cityName) params.set('city_name', cityName);
  return adminRequest<{ exists: boolean }>(
    `/api/institutions/check-cid-full-name?${params.toString()}`,
    auth,
  );
}

// 创建私权机构只返回最终链交易签名请求；管理员使用签名钱包签名由表单统一处理一次。
export async function createInstitution(
  auth: AdminAuth,
  routeSegment: string,
  input: CreateInstitutionInput,
): Promise<CreateInstitutionOutput> {
  const payload = buildInstitutionCreatePayload(input);
  return adminRequest<CreateInstitutionOutput>(`/api/private/${routeSegment}`, auth, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(payload),
  });
}

export async function uploadLegalRepresentativePhoto(
  auth: AdminAuth,
  file: File,
): Promise<LegalRepresentativePhoto> {
  const form = new FormData();
  form.append('file', file);
  return adminRequest<LegalRepresentativePhoto>(
    '/api/institutions/legal-representative/photo',
    auth,
    {
      method: 'POST',
      body: form,
    },
  );
}

export async function listPrivateInstitutions(
  auth: AdminAuth,
  routeSegment: string,
  query?: Omit<ListInstitutionsQuery, 'category'>,
): Promise<PageResult<InstitutionListRow>> {
  const params = new URLSearchParams();
  if (query?.private_type) params.set('private_type', query.private_type);
  if (query?.province_name) params.set('province_name', query.province_name);
  if (query?.city_name) params.set('city_name', query.city_name);
  if (query?.q && query.q.trim()) params.set('q', query.q.trim());
  if (query?.cursor) params.set('cursor', query.cursor);
  if (query?.page_size) params.set('page_size', String(query.page_size));
  return adminRequest<PageResult<InstitutionListRow>>(
    `/api/private/${routeSegment}?${params.toString()}`,
    auth,
  );
}

export async function getInstitution(
  auth: AdminAuth,
  cidNumber: string,
): Promise<InstitutionDetail> {
  return adminRequest<InstitutionDetail>(
    `/api/institutions/${encodeURIComponent(cidNumber)}`,
    auth,
  );
}

// 所属法人搜索。后端按 subjects/unincorporated_org 规则预过滤
// (分校→本市学校本部;其它 F→私法人全国 ∪ 公法人按层级地域),前端不再兜底过滤。
export async function searchParentInstitutions(
  auth: AdminAuth,
  q: string,
  opts: SearchParentsOptions,
): Promise<ParentInstitutionRow[]> {
  const params = new URLSearchParams({ q });
  params.set('f_institution', opts.fInstitution);
  params.set('province_name', opts.province_name);
  params.set('city_name', opts.city_name);
  if (opts.parentProperty) params.set('parent_property', opts.parentProperty);
  return adminRequest<ParentInstitutionRow[]>(
    `/api/institutions/search-parents?${params.toString()}`,
    auth,
  );
}

export function buildInstitutionUpdateSecurityPayload(
  cidNumber: string,
  input: UpdateInstitutionInput,
) {
  return {
    target: cidNumber,
    cid_number: cidNumber,
    cid_full_name: input.cid_full_name ?? null,
    parent_cid_number: input.parent_cid_number ?? null,
    family_name: input.family_name ?? null,
    given_name: input.given_name ?? null,
    legal_representative_cid_number: input.legal_representative_cid_number ?? null,
    legal_representative_photo_path: input.legal_representative_photo_path ?? null,
  };
}

// 更新机构属 PASSKEY_COLD_SIGN 操作:
// 授权 payload 只绑定后端实际校验字段;正式提交必须同时携带冷签 grant 与 Passkey assertion。
export async function updateInstitution(
  auth: AdminAuth,
  cidNumber: string,
  input: UpdateInstitutionInput,
  securityGrant: AdminSecurityGrantOutput,
): Promise<Institution> {
  const headers = await securityGrantSubmitHeaders(auth, securityGrant, {
    'content-type': 'application/json',
  });
  return adminRequest<Institution>(
    `/api/institutions/${encodeURIComponent(cidNumber)}`,
    auth,
    {
      method: 'PATCH',
      headers,
      body: JSON.stringify(input),
    },
  );
}
