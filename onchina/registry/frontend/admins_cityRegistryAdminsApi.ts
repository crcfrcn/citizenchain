import { type AdminAuth } from '../../platform/frontend/authentication_types';
import { adminHeaders, request } from '../../platform/frontend/helpers_http';
import { type FormInstance } from 'antd';
import { type CidCityItem } from '../../platform/frontend/china_api';
import { type AdminActionType } from '../../platform/frontend/admins_securityApi';
import { type InstitutionDetail } from '../../platform/frontend/subjects_api';
import { type PrepareInstitutionChainOutput } from '../../platform/frontend/admins_api';
// 市注册局管理员 API。
// 市注册局管理员列表、创建、删除都归入 admins 管理员功能目录。

export type CityRegistryAdminRow = {
  id: number;
  account_id: string;
  family_name: string;
  given_name: string;
  balance_fen?: string | null;
  institution_code: string;
  built_in: boolean;
  creator_account_id: string;
  creator_family_name: string;
  creator_given_name: string;
  created_at: string;
  city_name: string;
};

export async function listCityRegistryAdmins(auth: AdminAuth): Promise<CityRegistryAdminRow[]> {
  const data = await request<{ total: number; rows: CityRegistryAdminRow[] }>('/api/admin/city-registry-admins', {
    method: 'GET',
    headers: adminHeaders(auth),
  });
  return data.rows;
}

// 联邦注册局管理员视图共享工具函数与类型







export const MAX_CITY_REGISTRY_ADMINS_PER_CITY = 30;


/** 扫码目标类型 */
export type AccountScanTarget = null | 'city_registry';

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
  addCityRegistryLoading: boolean;

  accountScanTarget: AccountScanTarget;
  setAccountScanTarget: (v: AccountScanTarget) => void;

  addCityRegistryForm: FormInstance<{ city_registry_account: string; family_name: string; given_name: string; city_scope_city_name: string }>;

  onCreateCityRegistry: (values: { city_registry_account: string; family_name: string; given_name: string; city_name?: string; creator_account_id?: string }) => Promise<void>;
  onDeleteCityRegistry: (row: CityRegistryAdminRow) => void;
  runSecuredAction: <T = unknown>(actionType: AdminActionType, payload: unknown) => Promise<T>;

  /** 联邦注册局机构详情(全国唯一,scope-bypass 接口加载)。用于「联邦注册局」子 tab 详情页。 */
  federalRegistryDetail: InstitutionDetail | null;
  federalRegistryLoading: boolean;
  /** 当前活动市(selectedCity / 市注册局管理员 lockedCityName)对应的市注册局机构 cid_number。 */
  cityRegistryCid: string | null;
  cityRegistryLoading: boolean;
}

export type FederalRegistryAdminRow = {
  id: number;
  province_name: string;
  account_id: string;
  family_name: string;
  given_name: string;
  role_code: string;
  role_name: string;
  term_required: boolean;
  term_start: number;
  term_end: number;
  assignment_source: number;
  assignment_source_label: string;
  assignment_source_ref: string;
  balance_fen?: string | null;
  built_in: boolean;
  created_at: string;
  /** 最近一次更新时间,null 表示从未更新。 */
  updated_at?: string | null;
};

export type PrepareRegisterInstitutionAdminsInput = {
  cid_number: string;
  admins: InstitutionGovernanceAdminInput[];
};

export async function listFederalRegistryAdmins(auth: AdminAuth): Promise<FederalRegistryAdminRow[]> {
  return request<FederalRegistryAdminRow[]>('/api/admin/federal-registry-admins', {
    method: 'GET',
    headers: adminHeaders(auth),
  });
}

export async function prepareRegisterInstitutionAdmins(
  auth: AdminAuth,
  input: PrepareRegisterInstitutionAdminsInput,
): Promise<PrepareInstitutionChainOutput> {
  return request<PrepareInstitutionChainOutput>('/api/admin/institution/admins/register/prepare', {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      ...adminHeaders(auth),
    },
    body: JSON.stringify(input),
  });
}
