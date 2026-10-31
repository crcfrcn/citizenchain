import { type AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { adminHeaders, adminRequest, request } from '@gmb/onchina-platform/http';
import { type FormInstance } from 'antd';
import { type CidCityItem } from '@gmb/onchina-platform/codes/api';
import { passkeySubmitHeaders, type AdminActionType } from '@gmb/onchina-platform/authentication/security';
import { type InstitutionDetail } from '@gmb/onchina-platform/institutions/api';
import { type InstitutionGovernanceAdminInput, type PrepareInstitutionChainOutput } from '@gmb/onchina-platform/institutions/governance';
// 市注册局管理员 API。
// 市注册局管理员列表、创建、删除都归入 admins 管理员功能目录。

export type CityRegistryAdminRow = {
  id: number;
  accountId: string;
  familyName: string;
  givenName: string;
  balanceFen?: string | null;
  institutionCode: string;
  builtIn: boolean;
  creatorAccountId: string;
  creatorFamilyName: string;
  creatorGivenName: string;
  createdAt: string;
  cityName: string;
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

  addCityRegistryForm: FormInstance<{ cityRegistryAccount: string; familyName: string; givenName: string; cityScopeCityName: string }>;

  onCreateCityRegistry: (values: { cityRegistryAccount: string; familyName: string; givenName: string; cityName?: string; creatorAccountId?: string }) => Promise<void>;
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
  provinceName: string;
  accountId: string;
  familyName: string;
  givenName: string;
  roleCode: string;
  roleName: string;
  termRequired: boolean;
  termStart: number;
  termEnd: number;
  assignmentSource: number;
  assignmentSourceLabel: string;
  assignmentSourceRef: string;
  balanceFen?: string | null;
  builtIn: boolean;
  createdAt: string;
  /** 最近一次更新时间,null 表示从未更新。 */
  updatedAt?: string | null;
};

export type PrepareRegisterInstitutionAdminsInput = {
  cidNumber: string;
  admins: InstitutionGovernanceAdminInput[];
  /** 当前操作注册局管理员的岗位码，不是目标机构岗位。 */
  actorRoleCode: string;
};

export async function listFederalRegistryAdmins(auth: AdminAuth): Promise<FederalRegistryAdminRow[]> {
  return request<FederalRegistryAdminRow[]>('/api/admin/federal-registry-admins', {
    method: 'GET',
    headers: adminHeaders(auth),
  });
}

/** 登记管理员复用一次Passkey认证，原登记字段与钱包签名流程保持。 */
export async function prepareRegisterInstitutionAdmins(
  auth: AdminAuth,
  input: PrepareRegisterInstitutionAdminsInput,
): Promise<PrepareInstitutionChainOutput> {
  return adminRequest<PrepareInstitutionChainOutput>('/api/admin/institution/admins/register/prepare', auth, {
    method: 'POST',
    headers: await passkeySubmitHeaders(auth, { 'content-type': 'application/json' }),
    body: JSON.stringify(input),
  });
}
