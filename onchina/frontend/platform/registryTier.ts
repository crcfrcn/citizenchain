// 控制台注册局分层单点谓词(镜像后端 chain_runtime::is_tier1_registry / is_subordinate_registry)。
//
// 取代散落各视图的 `institution_code === 'FRG' / 'CREG'` 字面——分层判定收敛到此单点,
// 与后端机构码单源(TIER1_REGISTRY_CODE / TIER2_REGISTRY_CODE)对齐。前端只做 render-gating,
// 鉴权真源在后端。

/** Tier1 创世注册局机构码(本期 = 联邦注册局)。 */
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
