// 股权公司前端类型边界。固定 `S + GQ`,管理股东和出资关系。

import type { PrivateType } from '../../subjects/api';

export const COMPANY_PRIVATE_TYPE: PrivateType = 'COMPANY';
export const COMPANY_ROUTE_SEGMENT = 'company';
export const COMPANY_TITLE = '股权公司';

export interface CompanyProfileFields {
  identityCode: 'GQ';
  shareholderRole: 'EQUITY_SHAREHOLDER';
  hasLegalPersonality: true;
}
