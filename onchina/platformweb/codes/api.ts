// CID 元数据 API。这里承接省份、城市、机构码等跨页面选择项。

import { adminHeaders, request } from "../workspace/http";
import type { AdminAuth } from '../authentication/types';

export type CidInstitutionCodeItem = {
  institutionCode: string;
  institutionCodeLabel: string;
};

export type CidProvinceItem = {
  provinceName: string;
  provinceCode: string;
};

export type CidCityItem = {
  cityName: string;
  cityCode: string;
};

export type CidTownItem = {
  townName: string;
  townCode: string;
};

export type CidMetaResult = {
  institutionOptions: CidInstitutionCodeItem[];
  provinces: CidProvinceItem[];
  allProvinces: CidProvinceItem[];
  scopedProvinceName?: string | null;
};

export async function getCidMeta(auth: AdminAuth): Promise<CidMetaResult> {
  return request<CidMetaResult>('/api/admin/cid/meta', {
    method: 'GET',
    headers: adminHeaders(auth),
  });
}

export async function listCidCities(auth: AdminAuth, provinceName: string): Promise<CidCityItem[]> {
  const q = `?province_name=${encodeURIComponent(provinceName)}`;
  return request<CidCityItem[]>(`/api/admin/cid/china/cities${q}`, {
    method: 'GET',
    headers: adminHeaders(auth),
  });
}

export async function listCidTowns(
  auth: AdminAuth,
  provinceName: string,
  cityCode: string,
): Promise<CidTownItem[]> {
  const params = new URLSearchParams({ provinceName, cityCode });
  return request<CidTownItem[]>(`/api/admin/cid/china/towns?${params.toString()}`, {
    method: 'GET',
    headers: adminHeaders(auth),
  });
}
