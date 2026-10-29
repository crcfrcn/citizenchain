import { passkeySubmitHeaders } from '../../platform/frontend/admins_securityApi';
// 私权机构前端共用 API。六类私权机构必须在各自的 private_*_api.ts 中传入 routeSegment,
// 本文件只负责封装共用 HTTP、查重、证件照、详情和参与主体共享接口。

import type { AdminAuth } from '../../platform/frontend/authentication_types';
import {
  securityGrantSubmitHeaders,
  type AdminSecurityGrantOutput,
} from '../../platform/frontend/admins_securityApi';
import { adminRequest } from '../../platform/frontend/helpers_http';
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
} from '../../platform/frontend/subjects_api';
import { buildInstitutionCreatePayload } from '../../platform/frontend/subjects_api';

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
} from '../../platform/frontend/subjects_api';

export async function checkCidFullName(
  auth: AdminAuth,
  cidFullName: string,
  subjectProperty?: string,
  cityName?: string,
): Promise<{ exists: boolean }> {
  const params = new URLSearchParams({ cidFullName: cidFullName });
  if (subjectProperty) params.set('subjectProperty', subjectProperty);
  if (cityName) params.set('cityName', cityName);
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
      method: 'POST', headers: await passkeySubmitHeaders(auth),
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
  if (query?.privateType) params.set('privateType', query.privateType);
  if (query?.provinceName) params.set('provinceName', query.provinceName);
  if (query?.cityName) params.set('cityName', query.cityName);
  if (query?.q && query.q.trim()) params.set('q', query.q.trim());
  if (query?.cursor) params.set('cursor', query.cursor);
  if (query?.pageSize) params.set('pageSize', String(query.pageSize));
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

// 所属法人搜索。后端按 onchina/platform/institutions/subjects_unincorporated_org.rs 规则预过滤
// (分校→本市学校本部;其它 F→私法人全国 ∪ 公法人按层级地域),前端不再兜底过滤。
export async function searchParentInstitutions(
  auth: AdminAuth,
  q: string,
  opts: SearchParentsOptions,
): Promise<ParentInstitutionRow[]> {
  const params = new URLSearchParams({ q });
  params.set('fInstitution', opts.fInstitution);
  params.set('provinceName', opts.provinceName);
  params.set('cityName', opts.cityName);
  if (opts.parentProperty) params.set('parentProperty', opts.parentProperty);
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
    cidNumber: cidNumber,
    cidFullName: input.cidFullName ?? null,
    parentCidNumber: input.parentCidNumber ?? null,
    familyName: input.familyName ?? null,
    givenName: input.givenName ?? null,
    representativeCid: input.representativeCid ?? null,
    photoPath: input.photoPath ?? null,
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
