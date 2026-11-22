// 按后端规则识别联邦和下级注册局。

export const TIER1_REGISTRY_CODE = 'FRG';

/** Tier2 下级注册局机构码(本期 = 市注册局),由 Tier1 供给。 */
export const TIER2_REGISTRY_CODE = 'CREG';

/** 是否为 Tier1 创世注册局(本期 = 联邦注册局)。 */
export function isTier1Registry(institutionCode: string | null | undefined): boolean {
  return institutionCode === TIER1_REGISTRY_CODE;
}

/** 是否为 Tier2 下级注册局(本期 = 市注册局)。 */
export function isSubordinateRegistry(institutionCode: string | null | undefined): boolean {
  return institutionCode === TIER2_REGISTRY_CODE;
}
