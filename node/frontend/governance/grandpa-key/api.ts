import { invoke } from '../../tauri';
import type {
  GrandpaKeyChangeRequest,
  GrandpaKeyChangeStatus,
  GrandpaKeyChangeSubmitResult,
} from './types';

export const grandpaKeyChangeApi = {
  // 前端只调用节点暴露的三段式换钥命令，不在浏览器生成或持久化 GRANDPA 私钥。
  getStatus: () =>
    invoke<GrandpaKeyChangeStatus>('get_grandpa_key_change_status'),

  buildRequest: (
    actorCidNumber: string,
    signer_public_key: string,
    emergencyRecovery: boolean,
    unlockPassword: string,
  ) =>
    invoke<GrandpaKeyChangeRequest>('build_grandpa_key_change_request', {
      actor_cid_number: actorCidNumber,
      signer_public_key,
      emergency_recovery: emergencyRecovery,
      unlock_password: unlockPassword,
    }),

  submit: (
    requestId: string,
    expected_signer_public_key: string,
    expectedPayloadHash: string,
    signNonce: number,
    signBlockNumber: number,
    responseJson: string,
  ) =>
    invoke<GrandpaKeyChangeSubmitResult>('submit_grandpa_key_change', {
      request_id: requestId,
      expected_signer_public_key,
      expected_payload_hash: expectedPayloadHash,
      sign_nonce: signNonce,
      sign_block_number: signBlockNumber,
      response_json: responseJson,
    }),
};
