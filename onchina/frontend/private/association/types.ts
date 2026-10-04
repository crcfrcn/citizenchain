// 注册协会前端类型边界。机构码固定为 SFAS，盈利属性由每个实例显式选择。

import type { PrivateType } from '../../subjects/api';

export const ASSOCIATION_PRIVATE_TYPE: PrivateType = 'ASSOCIATION';
export const ASSOCIATION_ROUTE_SEGMENT = 'association';
export const ASSOCIATION_TITLE = '注册协会';

export interface AssociationProfileFields {
  identityCode: 'AS';
  p1: '0' | '1';
  memberRole: 'MEMBER';
  hasLegalPersonality: true;
}
