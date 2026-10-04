import { invoke } from '../../tauri';
import type { VoteSignRequestResult, VoteSubmitResult } from '../../governance/types';
import type {
  ClearingBankNodeOnChainInfo,
  ConnectivityTestReport,
  DecryptAdminRequestResult,
  DecryptedAdminInfo,
} from './types';

// 清算行 offchain 网络专用 Tauri API。机构身份只读命令归同目录 institution/api.ts。
export const offchainApi = {
  queryClearingBankNodeInfo: (cidNumber: string) =>
    invoke<ClearingBankNodeOnChainInfo | null>('query_clearing_bank_node_info', {
      cid_number: cidNumber,
    }),

  queryLocalPeerId: () => invoke<string>('query_local_peer_id'),

  testClearingBankEndpointConnectivity: (domain: string, port: number, expectedPeerId: string) =>
    invoke<ConnectivityTestReport>('test_clearing_bank_endpoint_connectivity', {
      domain,
      port,
      expected_peer_id: expectedPeerId,
    }),

  buildRegisterClearingBankRequest: (
    signer_public_key: string,
    actorCidNumber: string,
    peerId: string,
    rpcDomain: string,
    rpcPort: number,
  ) =>
    invoke<VoteSignRequestResult>('build_register_clearing_bank_request', {
      signer_public_key,
      actor_cid_number: actorCidNumber,
      peer_id: peerId,
      rpc_domain: rpcDomain,
      rpc_port: rpcPort,
    }),

  submitRegisterClearingBank: (
    requestId: string,
    expected_signer_public_key: string,
    expectedPayloadHash: string,
    actorCidNumber: string,
    peerId: string,
    rpcDomain: string,
    rpcPort: number,
    signNonce: number,
    signBlockNumber: number,
    responseJson: string,
  ) =>
    invoke<VoteSubmitResult>('submit_register_clearing_bank', {
      request_id: requestId,
      expected_signer_public_key,
      expected_payload_hash: expectedPayloadHash,
      actor_cid_number: actorCidNumber,
      peer_id: peerId,
      rpc_domain: rpcDomain,
      rpc_port: rpcPort,
      sign_nonce: signNonce,
      sign_block_number: signBlockNumber,
      response_json: responseJson,
    }),

  buildUpdateClearingBankEndpointRequest: (
    signer_public_key: string,
    actorCidNumber: string,
    newDomain: string,
    newPort: number,
  ) =>
    invoke<VoteSignRequestResult>('build_update_clearing_bank_endpoint_request', {
      signer_public_key,
      actor_cid_number: actorCidNumber,
      new_domain: newDomain,
      new_port: newPort,
    }),

  submitUpdateClearingBankEndpoint: (
    requestId: string,
    expected_signer_public_key: string,
    expectedPayloadHash: string,
    actorCidNumber: string,
    newDomain: string,
    newPort: number,
    signNonce: number,
    signBlockNumber: number,
    responseJson: string,
  ) =>
    invoke<VoteSubmitResult>('submit_update_clearing_bank_endpoint', {
      request_id: requestId,
      expected_signer_public_key,
      expected_payload_hash: expectedPayloadHash,
      actor_cid_number: actorCidNumber,
      new_domain: newDomain,
      new_port: newPort,
      sign_nonce: signNonce,
      sign_block_number: signBlockNumber,
      response_json: responseJson,
    }),

  buildUnregisterClearingBankRequest: (signer_public_key: string, actorCidNumber: string) =>
    invoke<VoteSignRequestResult>('build_unregister_clearing_bank_request', {
      signer_public_key,
      actor_cid_number: actorCidNumber,
    }),

  submitUnregisterClearingBank: (
    requestId: string,
    expected_signer_public_key: string,
    expectedPayloadHash: string,
    actorCidNumber: string,
    signNonce: number,
    signBlockNumber: number,
    responseJson: string,
  ) =>
    invoke<VoteSubmitResult>('submit_unregister_clearing_bank', {
      request_id: requestId,
      expected_signer_public_key,
      expected_payload_hash: expectedPayloadHash,
      actor_cid_number: actorCidNumber,
      sign_nonce: signNonce,
      sign_block_number: signBlockNumber,
      response_json: responseJson,
    }),

  buildDecryptAdminRequest: (signer_public_key: string, cidNumber: string) =>
    invoke<DecryptAdminRequestResult>('build_decrypt_admin_request', {
      signer_public_key,
      cid_number: cidNumber,
    }),

  verifyAndDecryptAdmin: (
    requestId: string,
    signer_public_key: string,
    expectedPayloadHash: string,
    responseJson: string,
  ) =>
    invoke<DecryptedAdminInfo>('verify_and_decrypt_admin', {
      request_id: requestId,
      signer_public_key,
      expected_payload_hash: expectedPayloadHash,
      response_json: responseJson,
    }),

  listDecryptedAdmins: (cidNumber: string) =>
    invoke<DecryptedAdminInfo[]>('list_decrypted_admins', { cid_number: cidNumber }),

  lockDecryptedAdmin: (signer_public_key: string) =>
    invoke<void>('lock_decrypted_admin', { signer_public_key }),
};
