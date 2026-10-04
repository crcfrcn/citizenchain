// 股份公司前端类型边界。固定 `S + GF`,管理股份和股东关系。

import type { PrivateType } from '../../subjects/api';

export const CORPORATION_PRIVATE_TYPE: PrivateType = 'CORPORATION';
export const CORPORATION_ROUTE_SEGMENT = 'corporation';
export const CORPORATION_TITLE = '股份公司';

export interface CorporationProfileFields {
  identityCode: 'GF';
  equityUnitLabel: '股份';
  shareholderRole: 'SHAREHOLDER';
  hasLegalPersonality: true;
}
