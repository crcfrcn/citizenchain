// 公民链基金会平台价格 API；通用 Bearer 请求仍复用 platform/frontend/helpers_http.ts。

import type { AdminAuth } from '../../platform/frontend/authentication_types';
import { adminRequest } from '../../platform/frontend/helpers_http';
import type {
  PlatformMembershipLevel,
  PlatformPrices,
  ProposePlatformPriceResult,
} from './membership_types';

export function getPlatformPrices(auth: AdminAuth): Promise<PlatformPrices> {
  return adminRequest<PlatformPrices>('/api/membership/platform-prices', auth);
}

export function proposePlatformPrice(
  auth: AdminAuth,
  proposerRoleCode: string,
  membershipLevel: PlatformMembershipLevel,
  newPriceFen: string,
): Promise<ProposePlatformPriceResult> {
  return adminRequest<ProposePlatformPriceResult>(
    '/api/membership/platform-prices/propose',
    auth,
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        proposer_role_code: proposerRoleCode,
        membership_level: membershipLevel,
        new_price_fen: newPriceFen,
      }),
    },
  );
}
