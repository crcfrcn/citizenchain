import { passkeySubmitHeaders } from '../../platform/frontend/admins_securityApi';
// 公权机构前端 API。市公安局已折叠为普通公权机构,与自动公权目录同走 gov 模块。
// 手动新增两能力:公权机构(G,ZF/LF/SF/JC,排除储备体系自动目录代码)+ 公权下属非法人(F,挂公法人);
// JY 教育机构归 education 模块。

import type { AdminAuth } from '../../platform/frontend/authentication_types';
import { adminRequest } from '../../platform/frontend/helpers_http';
import type {
  CreateInstitutionInput,
  CreateInstitutionOutput,
  InstitutionDetail,
  InstitutionListRow,
  LegalRepresentativePhoto,
  PageResult,
  ParentInstitutionRow,
  SearchParentsOptions,
} from '../../platform/frontend/subjects_api';
import { buildInstitutionCreatePayload } from '../../platform/frontend/subjects_api';


export type { CreateInstitutionOutput, InstitutionDetail } from '../../platform/frontend/subjects_api';

export interface ListOfficialInstitutionQuery {
  provinceName?: string;
  cityName?: string;
  q?: string;
  /** 机构码精确过滤(单源,如市注册局=CREG);省略=不过滤。 */
  institutionCode?: string;
  cursor?: string | null;
  pageSize?: number;
}

export async function listOfficialInstitutions(
  auth: AdminAuth,
  query?: ListOfficialInstitutionQuery,
): Promise<PageResult<InstitutionListRow>> {
  const params = new URLSearchParams();
  if (query?.provinceName) params.set('provinceName', query.provinceName);
  if (query?.cityName) params.set('cityName', query.cityName);
  if (query?.q && query.q.trim()) params.set('q', query.q.trim());
  if (query?.institutionCode && query.institutionCode.trim())
    params.set('institutionCode', query.institutionCode.trim());
  if (query?.cursor) params.set('cursor', query.cursor);
  if (query?.pageSize) params.set('pageSize', String(query.pageSize));
  const qs = params.toString();
  return adminRequest<PageResult<InstitutionListRow>>(
    qs ? `/api/institutions/gov?${qs}` : '/api/institutions/gov',
    auth,
  );
}

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

// 创建公权机构只返回最终链交易签名请求；管理员使用签名钱包签名由表单统一处理一次。
export async function createInstitution(
  auth: AdminAuth,
  input: CreateInstitutionInput,
): Promise<CreateInstitutionOutput> {
  const payload = buildInstitutionCreatePayload(input);
  return adminRequest<CreateInstitutionOutput>('/api/institutions/create', auth, {
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

// 所属法人搜索(公权入口 parentProperty=G → 本市市级/本省省级/国家级公法人)。
// 后端按 onchina/platform/institutions/subjects_unincorporated_org.rs 规则预过滤,前端不再兜底过滤。
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

export async function getInstitution(
  auth: AdminAuth,
  cidNumber: string,
): Promise<InstitutionDetail> {
  return adminRequest<InstitutionDetail>(
    `/api/institutions/${encodeURIComponent(cidNumber)}`,
    auth,
  );
}

/**
 * 联邦注册局机构详情(只读,绕过 scope)。
 * 联邦注册局是全国唯一机构(位于中枢省),其它联邦注册局管理员被普通 getInstitution 的 scope 拦截,
 * 故走专用只读接口。返回结构与 getInstitution 完全一致。
 */
export async function getFederalRegistry(
  auth: AdminAuth,
): Promise<InstitutionDetail> {
  return adminRequest<InstitutionDetail>(
    '/api/institutions/federal-registry',
    auth,
  );
}
