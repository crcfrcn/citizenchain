// 公益组织前端类型边界。固定 `S + GY`,非营利法人。

import type { PrivateType } from '../../subjects/api';

export const WELFARE_PRIVATE_TYPE: PrivateType = 'WELFARE';
export const WELFARE_ROUTE_SEGMENT = 'welfare';
export const WELFARE_TITLE = '公益组织';

export interface WelfareProfileFields {
  identityCode: 'GY';
  p1: '0';
  purposeLabel: '公益目的';
  hasLegalPersonality: true;
}
