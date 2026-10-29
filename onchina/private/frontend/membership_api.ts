// 公民链基金会平台价格 API；通用 Bearer 请求仍复用 platform/frontend/helpers_http.ts。

import type { AdminAuth } from '../../platform/frontend/authentication_types';
import { adminRequest } from '../../platform/frontend/helpers_http';
import { passkeySubmitHeaders } from '../../platform/frontend/admins_securityApi';
import type {
  PlatformMembershipLevel,
  PlatformPrices,
  ProposePlatformPriceResult,
} from './membership_types';

export function getPlatformPrices(auth: AdminAuth): Promise<PlatformPrices> {
  return adminRequest<PlatformPrices>('/api/membership/platform-prices', auth);
}

/** 调价提案先完成一次现有Passkey认证，原金额字段与钱包签名流程保持。 */
export async function proposePlatformPrice(
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
      headers: await passkeySubmitHeaders(auth, { 'content-type': 'application/json' }),
      body: JSON.stringify({
        proposerRoleCode: proposerRoleCode,
        membershipLevel: membershipLevel,
        newPriceFen: newPriceFen,
      }),
    },
  );
}
