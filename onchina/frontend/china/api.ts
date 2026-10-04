// CID 元数据 API。这里承接省份、城市、机构码等跨页面选择项。

import { adminHeaders, request } from '../utils/http';
import type { AdminAuth } from '../auth/types';

export type CidInstitutionCodeItem = {
  institution_code: string;
  institution_code_label: string;
};

export type CidProvinceItem = {
  province_name: string;
  province_code: string;
};

export type CidCityItem = {
  city_name: string;
  city_code: string;
};

export type CidTownItem = {
  town_name: string;
  town_code: string;
};

export type CidMetaResult = {
  institution_options: CidInstitutionCodeItem[];
  provinces: CidProvinceItem[];
  all_provinces: CidProvinceItem[];
  scoped_province_name?: string | null;
};

export async function getCidMeta(auth: AdminAuth): Promise<CidMetaResult> {
  return request<CidMetaResult>('/api/admin/cid/meta', {
    method: 'GET',
    headers: adminHeaders(auth),
  });
}

export async function listCidCities(auth: AdminAuth, province_name: string): Promise<CidCityItem[]> {
  const q = `?province_name=${encodeURIComponent(province_name)}`;
  return request<CidCityItem[]>(`/api/admin/cid/china/cities${q}`, {
    method: 'GET',
    headers: adminHeaders(auth),
  });
}

export async function listCidTowns(
  auth: AdminAuth,
  province_name: string,
  city_code: string,
): Promise<CidTownItem[]> {
  const params = new URLSearchParams({ province_name, city_code });
  return request<CidTownItem[]>(`/api/admin/cid/china/towns?${params.toString()}`, {
    method: 'GET',
    headers: adminHeaders(auth),
  });
}
