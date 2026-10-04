// 公权机构前端 API。市公安局已折叠为普通公权机构,与自动公权目录同走 gov 模块。
// 手动新增两能力:公权机构(G,ZF/LF/SF/JC,排除储备体系自动目录代码)+ 公权下属非法人(F,挂公法人);
// JY 教育机构归 education 模块。

import type { AdminAuth } from '../auth/types';
import { adminRequest } from '../utils/http';
import type {
  CreateInstitutionInput,
  CreateInstitutionOutput,
  InstitutionDetail,
  InstitutionListRow,
  LegalRepresentativePhoto,
  PageResult,
  ParentInstitutionRow,
  SearchParentsOptions,
} from '../subjects/api';
import { buildInstitutionCreatePayload } from '../subjects/api';


export type { CreateInstitutionOutput, InstitutionDetail } from '../subjects/api';

export interface ListOfficialInstitutionQuery {
  province_name?: string;
  city_name?: string;
  q?: string;
  /** 机构码精确过滤(单源,如市注册局=CREG);省略=不过滤。 */
  institution_code?: string;
  cursor?: string | null;
  page_size?: number;
}

export async function listOfficialInstitutions(
  auth: AdminAuth,
  query?: ListOfficialInstitutionQuery,
): Promise<PageResult<InstitutionListRow>> {
  const params = new URLSearchParams();
  if (query?.province_name) params.set('province_name', query.province_name);
  if (query?.city_name) params.set('city_name', query.city_name);
  if (query?.q && query.q.trim()) params.set('q', query.q.trim());
  if (query?.institution_code && query.institution_code.trim())
    params.set('institution_code', query.institution_code.trim());
  if (query?.cursor) params.set('cursor', query.cursor);
  if (query?.page_size) params.set('page_size', String(query.page_size));
  const qs = params.toString();
  return adminRequest<PageResult<InstitutionListRow>>(
    qs ? `/api/institutions/gov?${qs}` : '/api/institutions/gov',
    auth,
  );
}

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
      method: 'POST',
      body: form,
    },
  );
}

// 所属法人搜索(公权入口 parentProperty=G → 本市市级/本省省级/国家级公法人)。
// 后端按 subjects/unincorporated_org 规则预过滤,前端不再兜底过滤。
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
