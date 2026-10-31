import { passkeySubmitHeaders } from '@gmb/onchina-platform/authentication/security';
// 教育机构前端 API。JY 教育机构统一从这里调用后端:
// 市详情确定性市公民教育委员会直接列表展示,学校和 F+JY 非法人教育机构按精确搜索返回。

import type { AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { adminRequest } from '@gmb/onchina-platform/http';
import type {
  CreateInstitutionInput,
  CreateInstitutionOutput,
  InstitutionListRow,
  LegalRepresentativePhoto,
  ListInstitutionsQuery,
  PageResult,
  ParentInstitutionRow,
  SearchParentsOptions,
} from '@gmb/onchina-platform/institutions/api';
import { buildInstitutionCreatePayload } from '@gmb/onchina-platform/institutions/api';

export type {
  CreateInstitutionInput,
  CreateInstitutionOutput,
  InstitutionListRow,
  LegalRepresentativePhoto,
  PageResult,
} from '@gmb/onchina-platform/institutions/api';

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
      method: 'POST', headers: await passkeySubmitHeaders(auth),
      body: form,
    },
  );
}

// 所属法人搜索(分校模式 f_institution=JY → 只返回本市学校本部)。
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

export async function listEducationInstitutions(
  auth: AdminAuth,
  query?: Omit<ListInstitutionsQuery, 'category'>,
): Promise<PageResult<InstitutionListRow>> {
  const params = new URLSearchParams();
  // EDUCATION_FORM 是列表过滤维度(后端 InstitutionListFilter),不是存储 category
  params.set('category', 'EDUCATION_FORM');
  if (query?.provinceName) params.set('provinceName', query.provinceName);
  if (query?.cityName) params.set('cityName', query.cityName);
  if (query?.q && query.q.trim()) params.set('q', query.q.trim());
  if (query?.cursor) params.set('cursor', query.cursor);
  if (query?.pageSize) params.set('pageSize', String(query.pageSize));
  return adminRequest<PageResult<InstitutionListRow>>(
    `/api/institutions/list?${params.toString()}`,
    auth,
  );
}
