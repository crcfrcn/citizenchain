import { invoke } from '../tauri';
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
    signerPublicKey: string,
    emergencyRecovery: boolean,
    unlockPassword: string,
  ) =>
    invoke<GrandpaKeyChangeRequest>('build_grandpa_key_change_request', {
      actorCidNumber: actorCidNumber,
      signerPublicKey,
      emergencyRecovery: emergencyRecovery,
      unlockPassword: unlockPassword,
    }),

  submit: (
    requestId: string,
    expectedPublicKey: string,
    expectedPayloadHash: string,
    signNonce: number,
    signBlockNumber: number,
    responseJson: string,
  ) =>
    invoke<GrandpaKeyChangeSubmitResult>('submit_grandpa_key_change', {
      requestId: requestId,
      expectedPublicKey,
      expectedPayloadHash: expectedPayloadHash,
      signNonce: signNonce,
      signBlockNumber: signBlockNumber,
      responseJson: responseJson,
    }),
};
