// 公民链基金会平台价格 API；通用 Bearer 请求仍复用 utils/http.ts。

import type { AdminAuth } from '../auth/types';
import { adminRequest } from '../utils/http';
import type {
  PlatformMembershipLevel,
  PlatformPrices,
  ProposePlatformPriceResult,
} from './types';

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
