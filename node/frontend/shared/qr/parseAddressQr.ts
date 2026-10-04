// 解析「扫码识别账户」二维码,用于治理提案的收款地址、手续费地址与安全基金地址。
//
// 唯一事实源：citizenchain/crates/qr-protocol/registry.json
// 当前入口用于地址框：只接受能明确提供账户的用户码(k=3)与账户码(k=5)。
// 其它码型由当前业务入口拒绝；扫码内容不存在旧格式兜底。

import { accountIdToSs58 } from '../ss58';
import {
  parseQrEnvelope,
  QrParseError,
  type AccountIdCodeBody,
  type UserContactBody,
} from './citizenQr';

export type AddressScanResult = {
  ss58_address: string;
};

export function parseAddressQr(raw: string): AddressScanResult {
  const trimmed = raw.trim();
  let env;
  try {
    env = parseQrEnvelope(trimmed);
  } catch (error) {
    const message = error instanceof QrParseError ? error.message : String(error);
    throw new Error(`二维码解析失败: ${message}`);
  }

  if (env.kind === 'account_id_code') {
    const body = env.body as AccountIdCodeBody;
    return { ss58_address: accountIdToSs58(body.account_id) };
  }
  if (env.kind === 'user_contact') {
    const body = env.body as UserContactBody;
    return { ss58_address: accountIdToSs58(body.account_id) };
  }
  throw new Error('此地址框只接受用户码或账户码');
}
