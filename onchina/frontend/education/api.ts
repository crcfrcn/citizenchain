// 教育机构前端 API。JY 教育机构统一从这里调用后端:
// 市详情确定性市公民教育委员会直接列表展示,学校和 F+JY 非法人教育机构按精确搜索返回。

import type { AdminAuth } from '../auth/types';
import { adminRequest } from '../utils/http';
import type {
  CreateInstitutionInput,
  CreateInstitutionOutput,
  InstitutionListRow,
  LegalRepresentativePhoto,
  ListInstitutionsQuery,
  PageResult,
  ParentInstitutionRow,
  SearchParentsOptions,
} from '../subjects/api';
import { buildInstitutionCreatePayload } from '../subjects/api';

export type {
  CreateInstitutionInput,
  CreateInstitutionOutput,
  InstitutionListRow,
  LegalRepresentativePhoto,
  PageResult,
} from '../subjects/api';

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

// 创建教育机构只返回最终链交易签名请求；管理员使用签名钱包签名由表单统一处理一次。
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

// 所属法人搜索(分校模式 f_institution=JY → 只返回本市学校本部)。
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

export async function listEducationInstitutions(
  auth: AdminAuth,
  query?: Omit<ListInstitutionsQuery, 'category'>,
): Promise<PageResult<InstitutionListRow>> {
  const params = new URLSearchParams();
  // EDUCATION_FORM 是列表过滤维度(后端 InstitutionListFilter),不是存储 category
  params.set('category', 'EDUCATION_FORM');
  if (query?.province_name) params.set('province_name', query.province_name);
  if (query?.city_name) params.set('city_name', query.city_name);
  if (query?.q && query.q.trim()) params.set('q', query.q.trim());
  if (query?.cursor) params.set('cursor', query.cursor);
  if (query?.page_size) params.set('page_size', String(query.page_size));
  return adminRequest<PageResult<InstitutionListRow>>(
    `/api/institutions/list?${params.toString()}`,
    auth,
  );
}
