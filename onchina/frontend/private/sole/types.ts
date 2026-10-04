// 个体经营前端类型边界。固定 `F + GT`,无法人资格。

import type { PrivateType } from '../../subjects/api';

export const SOLE_PRIVATE_TYPE: PrivateType = 'SOLE';
export const SOLE_ROUTE_SEGMENT = 'sole';
export const SOLE_TITLE = '个体经营';

export interface SoleProfileFields {
  responsibleRole: 'RESPONSIBLE_PERSON';
  hasLegalPersonality: false;
  identityCode: 'GT';
}
