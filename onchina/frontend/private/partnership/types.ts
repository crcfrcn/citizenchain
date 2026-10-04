// 合伙企业前端类型边界。无限合伙=`F+GP`,有限合伙=`S+LP`。

import type { PartnershipKind, PrivateType } from '../../subjects/api';

export const PARTNERSHIP_PRIVATE_TYPE: PrivateType = 'PARTNERSHIP';
export const PARTNERSHIP_ROUTE_SEGMENT = 'partnership';
export const PARTNERSHIP_TITLE = '合伙企业';

export const PARTNERSHIP_KIND_OPTIONS: Array<{ value: PartnershipKind; label: string }> = [
  { value: 'GENERAL', label: '无限合伙' },
  { value: 'LIMITED', label: '有限合伙' },
];

export interface PartnershipProfileFields {
  partnershipKind: PartnershipKind;
  identityCode: 'GP' | 'LP';
  partnerRoles: Array<'GENERAL_PARTNER' | 'LIMITED_PARTNER'>;
}
