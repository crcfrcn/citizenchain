import { invoke } from "../client/tauri";
import type { VoteSignRequestResult, VoteSubmitResult } from "../client/signing";


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
    signerPublicKey: string,
    actorCidNumber: string,
    wasmPath: string,
    reason: string,
    powParams: PowDifficultyParams,
  ) =>
    invoke<ProposeUpgradeRequestResult>('build_propose_upgrade_request', {
      signerPublicKey,
      actorCidNumber: actorCidNumber,
      wasmPath: wasmPath,
      reason,
      powParams: powParams,
    }),
  submitProposeUpgrade: (
    requestId: string,
    expectedPublicKey: string,
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
      requestId: requestId,
      expectedPublicKey,
      expectedPayloadHash: expectedPayloadHash,
      actorCidNumber: actorCidNumber,
      wasmPath: wasmPath,
      reason,
      powParams: powParams,
      signNonce: signNonce,
      signBlockNumber: signBlockNumber,
      responseJson: responseJson,
    }),
};
