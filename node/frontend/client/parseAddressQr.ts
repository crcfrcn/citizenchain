import { accountIdToSs58 } from "./ss58";
import { parseQrEnvelope, QrParseError, type AccountIdCodeBody, type UserContactBody } from "./citizenQr";


export type AddressScanResult = {
  ss58Address: string;
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
    return { ss58Address: accountIdToSs58(body.account_id) };
  }
  if (env.kind === 'user_contact') {
    const body = env.body as UserContactBody;
    return { ss58Address: accountIdToSs58(body.account_id) };
  }
  throw new Error('此地址框只接受用户码或账户码');
}
