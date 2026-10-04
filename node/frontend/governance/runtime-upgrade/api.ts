import { invoke } from '../../tauri';
import type {
  VoteSignRequestResult,
  VoteSubmitResult,
} from '../types';

export type ProposeUpgradeRequestResult = VoteSignRequestResult;
export type PowDifficultyParams = {
  paramsVersion: number;
  algorithmVersion: number;
  targetBlockTimeMs: number;
  adjustmentInterval: number;
  maxAdjustUpFactor: number;
  maxAdjustDownDivisor: number;
};

// 协议升级专用 Tauri API。这里只提交业务提案，投票流程统一交给投票引擎。
export const runtimeUpgradeApi = {
  getPowDifficultyParams: () =>
    invoke<PowDifficultyParams>('get_pow_difficulty_params'),
  buildProposeUpgradeRequest: (
    signer_public_key: string,
    actorCidNumber: string,
    wasmPath: string,
    reason: string,
    powParams: PowDifficultyParams,
  ) =>
    invoke<ProposeUpgradeRequestResult>('build_propose_upgrade_request', {
      signer_public_key,
      actor_cid_number: actorCidNumber,
      wasm_path: wasmPath,
      reason,
      pow_params: powParams,
    }),
  submitProposeUpgrade: (
    requestId: string,
    expected_signer_public_key: string,
    expectedPayloadHash: string,
    actorCidNumber: string,
    wasmPath: string,
    reason: string,
    powParams: PowDifficultyParams,
    signNonce: number,
    signBlockNumber: number,
    responseJson: string,
  ) =>
    invoke<VoteSubmitResult>('submit_propose_upgrade', {
      request_id: requestId,
      expected_signer_public_key,
      expected_payload_hash: expectedPayloadHash,
      actor_cid_number: actorCidNumber,
      wasm_path: wasmPath,
      reason,
      pow_params: powParams,
      sign_nonce: signNonce,
      sign_block_number: signBlockNumber,
      response_json: responseJson,
    }),
};
