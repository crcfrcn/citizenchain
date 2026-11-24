// 调用清算行端点检测、节点登记及密钥解锁原生命令。

import { invoke } from "../client/tauri";
import type { VoteSignRequestResult, VoteSubmitResult } from "../client/signing";
import type { ClearingBankNodeOnChainInfo, ConnectivityTestReport, ClearingDecryptRequest, ClearingDecryptInfo } from "./types";


// 清算行 offchain 网络专用 Tauri API。机构身份只读命令归同目录 institution/api.ts。
export const offchainApi = {
  queryClearingBankNodeInfo: (cidNumber: string) =>
    invoke<ClearingBankNodeOnChainInfo | null>('query_clearing_bank_node_info', {
      cidNumber: cidNumber,
    }),

  queryLocalPeerId: () => invoke<string>('query_local_peer_id'),

  testClearingBankEndpointConnectivity: (domain: string, port: number, expectedPeerId: string) =>
    invoke<ConnectivityTestReport>('test_clearing_bank_endpoint_connectivity', {
      domain,
      port,
      expectedPeerId: expectedPeerId,
    }),

  buildRegisterClearingBankRequest: (
    signerPublicKey: string,
    actorCidNumber: string,
    actorRoleCode: string,
    peerId: string,
    rpcDomain: string,
    rpcPort: number,
  ) =>
    invoke<VoteSignRequestResult>('build_register_clearing_bank_request', {
      signerPublicKey,
      actorCidNumber: actorCidNumber,
      actorRoleCode,
      peerId: peerId,
      rpcDomain: rpcDomain,
      rpcPort: rpcPort,
    }),

  submitRegisterClearingBank: (
    requestId: string,
    expectedPublicKey: string,
    expectedPayloadHash: string,
    actorCidNumber: string,
    actorRoleCode: string,
    peerId: string,
    rpcDomain: string,
    rpcPort: number,
    signNonce: number,
    signBlockNumber: number,
    responseJson: string,
  ) =>
    invoke<VoteSubmitResult>('submit_register_clearing_bank', {
      requestId: requestId,
      expectedPublicKey,
      expectedPayloadHash: expectedPayloadHash,
      actorCidNumber: actorCidNumber,
      actorRoleCode,
      peerId: peerId,
      rpcDomain: rpcDomain,
      rpcPort: rpcPort,
      signNonce: signNonce,
      signBlockNumber: signBlockNumber,
      responseJson: responseJson,
    }),

  buildUpdateClearingBankEndpointRequest: (
    signerPublicKey: string,
    actorCidNumber: string,
    actorRoleCode: string,
    newDomain: string,
    newPort: number,
  ) =>
    invoke<VoteSignRequestResult>('build_update_clearing_bank_endpoint_request', {
      signerPublicKey,
      actorCidNumber: actorCidNumber,
      actorRoleCode,
      newDomain: newDomain,
      newPort: newPort,
    }),

  submitUpdateClearingBankEndpoint: (
    requestId: string,
    expectedPublicKey: string,
    expectedPayloadHash: string,
    actorCidNumber: string,
    actorRoleCode: string,
    newDomain: string,
    newPort: number,
    signNonce: number,
    signBlockNumber: number,
    responseJson: string,
  ) =>
    invoke<VoteSubmitResult>('submit_update_clearing_bank_endpoint', {
      requestId: requestId,
      expectedPublicKey,
      expectedPayloadHash: expectedPayloadHash,
      actorCidNumber: actorCidNumber,
      actorRoleCode,
      newDomain: newDomain,
      newPort: newPort,
      signNonce: signNonce,
      signBlockNumber: signBlockNumber,
      responseJson: responseJson,
    }),

  buildUnregisterClearingBankRequest: (signerPublicKey: string, actorCidNumber: string, actorRoleCode: string) =>
    invoke<VoteSignRequestResult>('build_unregister_clearing_bank_request', {
      signerPublicKey,
      actorCidNumber: actorCidNumber,
      actorRoleCode,
    }),

  submitUnregisterClearingBank: (
    requestId: string,
    expectedPublicKey: string,
    expectedPayloadHash: string,
    actorCidNumber: string,
    actorRoleCode: string,
    signNonce: number,
    signBlockNumber: number,
    responseJson: string,
  ) =>
    invoke<VoteSubmitResult>('submit_unregister_clearing_bank', {
      requestId: requestId,
      expectedPublicKey,
      expectedPayloadHash: expectedPayloadHash,
      actorCidNumber: actorCidNumber,
      actorRoleCode,
      signNonce: signNonce,
      signBlockNumber: signBlockNumber,
      responseJson: responseJson,
    }),

  buildClearingDecryptRequest: (signerPublicKey: string, cidNumber: string) =>
    invoke<ClearingDecryptRequest>('build_clearing_decrypt_request', {
      signerPublicKey,
      cidNumber: cidNumber,
    }),

  verifyClearingDecrypt: (
    requestId: string,
    signerPublicKey: string,
    expectedPayloadHash: string,
    responseJson: string,
  ) =>
    invoke<ClearingDecryptInfo>('verify_clearing_decrypt', {
      requestId: requestId,
      signerPublicKey,
      expectedPayloadHash: expectedPayloadHash,
      responseJson: responseJson,
    }),

  listClearingDecrypt: (cidNumber: string) =>
    invoke<ClearingDecryptInfo[]>('list_clearing_decrypt', { cidNumber: cidNumber }),

  lockClearingDecrypt: (signerPublicKey: string) =>
    invoke<void>('lock_clearing_decrypt', { signerPublicKey }),
};
