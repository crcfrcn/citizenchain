// 联邦注册局管理员视图共享工具函数与类型

import type { FormInstance } from 'antd';
import type { CidCityItem } from '../china/api';
import type { CityRegistryAdminRow } from './cityRegistryAdminsApi';
import type { FederalRegistryAdminRow } from './api';
import type { AdminActionType } from './securityApi';
import type { InstitutionDetail } from '../subjects/api';

export const MAX_CITY_REGISTRY_ADMINS_PER_CITY = 30;

/** 比较两个 hex 公钥是否相同(忽略大小写和 0x 前缀) */
export function sameHexAccount(a: string | null | undefined, b: string | null | undefined): boolean {
  if (!a || !b) return false;
  return a.trim().replace(/^0x/i, '').toLowerCase() === b.trim().replace(/^0x/i, '').toLowerCase();
}

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
