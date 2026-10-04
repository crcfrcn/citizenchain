// 前端镜像后端 scope::rules::VisibleScope,按机构行政层级(admin_level)派生可见域。
//
// 五档(与后端 get_visible_scope 逐档一致):
//   - 全国(NATIONAL,部委等)        → 不限省/市/镇
//   - 省级(PROVINCE / 联邦注册局)   → 本省,所有市(skipProvinceList=true 直进本省)
//   - 市级(CITY)                    → 本市,所有镇(skipCityList=true 直进本市)
//   - 镇级(TOWN)                    → 本镇(skipTownList=true 直进本镇)
//   - 自机构(私权法人/非法人,无层级)→ 暂沿用本市范围
//
// 鉴权真源在后端;本 hook 只服务前端 UX:写按钮置灰(canWrite*)+ 进 tab 跳级导航(skip*)。

import { useMemo } from 'react';
import type { AdminAuth } from '../auth/types';
import { isTier1Registry } from '../platform/registryTier';

export interface VisibleScope {
  /** 全国可见(NATIONAL 档);为 true 时写权限不再按 lockedProvinceName 收紧。 */
  nationwide: boolean;
  /** 可见省份列表。空数组 = "无可见省 / 全国"。 */
  provinces: string[];
  /** 可见市列表。空数组 = "不限市"(全国/省级)。 */
  cities: string[];
  /** 可见镇列表。空数组 = "不限镇"(全国/省级/市级)。 */
  towns: string[];
  /** 是否可以增删改(本辖区内才有写权限)。 */
  canWrite: boolean;
  /** 进 tab 时跳过省列表直接进入省详情(省级及以下)。 */
  skipProvinceList: boolean;
  /** 进 tab 时跳过市列表直接进入市详情(市级及以下)。 */
  skipCityList: boolean;
  /** 进 tab 时跳过镇列表直接进入镇详情(仅镇级)。 */
  skipTownList: boolean;
  /** 锁定的省名称(省级及以下必填)。 */
  lockedProvinceName: string | null;
  /** 锁定的市名称(市级及以下必填)。 */
  lockedCityName: string | null;
  /** 锁定的镇名称(仅镇级必填)。 */
  lockedTownName: string | null;

  /** 判断某省是否在可见范围内。 */
  includesProvince(province_name: string): boolean;
  /** 判断某市是否在可见范围内。 */
  includesCity(city_name: string): boolean;
  /** 判断某镇是否在可见范围内。 */
  includesTown(town_name: string): boolean;
  /** 判断某省是否允许写操作(跨辖区一律置灰)。 */
  canWriteProvince(province_name: string): boolean;
  /** 判断某市是否允许写操作(跨市置灰;省级本省内任意市可写)。 */
  canWriteCity(province_name: string, city_name: string): boolean;
  /** 判断某镇是否允许写操作(跨镇置灰;市级及以上本辖区内任意镇可写)。 */
  canWriteTown(province_name: string, city_name: string, town_name: string): boolean;
}

type ScopeBase = Omit<
  VisibleScope,
  | 'includesProvince'
  | 'includesCity'
  | 'includesTown'
  | 'canWriteProvince'
  | 'canWriteCity'
  | 'canWriteTown'
>;

function makeScope(base: ScopeBase): VisibleScope {
  return {
    ...base,
    // 前端 Dashboard 全局视图:任意行政区都"可见",只是写权限按 locked* 收紧。
    includesProvince(_province: string) {
      return true;
    },
    includesCity(_city: string) {
      return true;
    },
    includesTown(_town: string) {
      return true;
    },
    canWriteProvince(province_name: string) {
      if (!base.canWrite) return false;
      if (base.nationwide) return true;
      if (!base.lockedProvinceName) return false;
      return province_name === base.lockedProvinceName;
    },
    canWriteCity(province_name: string, city_name: string) {
      if (!base.canWrite) return false;
      if (base.nationwide) return true;
      if (!base.lockedProvinceName || province_name !== base.lockedProvinceName) return false;
      // 省级本省内任意市;市级及以下必须等于自己 lockedCityName。
      if (base.lockedCityName && city_name !== base.lockedCityName) return false;
      return true;
    },
    canWriteTown(province_name: string, city_name: string, town_name: string) {
      if (!this.canWriteCity(province_name, city_name)) return false;
      // 市级及以上本市内任意镇;镇级必须等于自己 lockedTownName。
      if (base.lockedTownName && town_name !== base.lockedTownName) return false;
      return true;
    },
  };
}

export function normalizeScopeProvinceName(value: string | null | undefined): string | null {
  const province = value?.trim();
  if (!province || province === '全国') return null;
  return province;
}

function noAuthScope(): VisibleScope {
  return makeScope({
    nationwide: false,
    provinces: ['__NO_AUTH__'],
    cities: ['__NO_AUTH__'],
    towns: ['__NO_AUTH__'],
    canWrite: false,
    skipProvinceList: false,
    skipCityList: false,
    skipTownList: false,
    lockedProvinceName: null,
    lockedCityName: null,
    lockedTownName: null,
  });
}

function nationalScope(): VisibleScope {
  return makeScope({
    nationwide: true,
    provinces: [],
    cities: [],
    towns: [],
    canWrite: true,
    skipProvinceList: false,
    skipCityList: false,
    skipTownList: false,
    lockedProvinceName: null,
    lockedCityName: null,
    lockedTownName: null,
  });
}

function provinceScope(auth: AdminAuth): VisibleScope {
  const province_name = normalizeScopeProvinceName(auth.scope_province_name);
  return makeScope({
    nationwide: false,
    provinces: province_name ? [province_name] : [],
    cities: [],
    towns: [],
    // 缺省域是后端投影错误,前端进入只读错误态,不造伪省名。
    canWrite: !!province_name,
    skipProvinceList: true,
    skipCityList: false,
    skipTownList: false,
    lockedProvinceName: province_name,
    lockedCityName: null,
    lockedTownName: null,
  });
}

function cityScope(auth: AdminAuth): VisibleScope {
  const province_name = normalizeScopeProvinceName(auth.scope_province_name);
  const city_name = auth.scope_city_name?.trim() || null;
  return makeScope({
    nationwide: false,
    provinces: province_name ? [province_name] : [],
    cities: city_name ? [city_name] : [],
    towns: [],
    canWrite: !!province_name && !!city_name,
    skipProvinceList: true,
    skipCityList: true,
    skipTownList: false,
    lockedProvinceName: province_name,
    lockedCityName: city_name,
    lockedTownName: null,
  });
}

function townScope(auth: AdminAuth): VisibleScope {
  const province_name = normalizeScopeProvinceName(auth.scope_province_name);
  const city_name = auth.scope_city_name?.trim() || null;
  const town_name = auth.scope_town_name?.trim() || null;
  return makeScope({
    nationwide: false,
    provinces: province_name ? [province_name] : [],
    cities: city_name ? [city_name] : [],
    towns: town_name ? [town_name] : [],
    canWrite: !!province_name && !!city_name && !!town_name,
    skipProvinceList: true,
    skipCityList: true,
    skipTownList: true,
    lockedProvinceName: province_name,
    lockedCityName: city_name,
    lockedTownName: town_name,
  });
}

export function useScope(auth: AdminAuth | null): VisibleScope {
  return useMemo<VisibleScope>(() => {
    if (!auth) return noAuthScope();
    // Tier1 创世注册局特判:admin_level 虽为 NATIONAL,但管理员按省分区(每节点单省)→ 省级范围。
    if (isTier1Registry(auth.institution_code)) return provinceScope(auth);
    switch (auth.admin_level) {
      case 'NATIONAL':
        return nationalScope();
      case 'PROVINCE':
        return provinceScope(auth);
      case 'CITY':
        return cityScope(auth);
      case 'TOWN':
        return townScope(auth);
      default:
        // 私权法人/非法人(无 admin_level):暂沿用本市范围(与后端决策一致)。
        return cityScope(auth);
    }
  }, [
    auth?.institution_code,
    auth?.admin_level,
    auth?.scope_province_name,
    auth?.scope_city_name,
    auth?.scope_town_name,
  ]);
}
