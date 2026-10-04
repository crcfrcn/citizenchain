import type { VoteSubmitResult } from '../types';

export type GrandpaChangeKind = 'routine_rotation' | 'emergency_recovery';

// 请求字段完整绑定候选密钥证明窗口，提交时不得只凭 requestId 省略其余校验值。
export type GrandpaKeyChangeRequest = {
  requestJson: string;
  requestId: string;
  expectedPayloadHash: string;
  signNonce: number;
  signBlockNumber: number;
  oldPublicKey: string;
  newPublicKey: string;
  proofNonce: number;
  proofExpiresAt: number;
  changeKind: GrandpaChangeKind;
};

export type GrandpaKeyChangeStatus = {
  pending: boolean;
  actorCidNumber: string | null;
  oldPublicKey: string | null;
  newPublicKey: string | null;
  changeKind: GrandpaChangeKind | null;
  txHash: string | null;
};

export type GrandpaKeyChangeSubmitResult = VoteSubmitResult;
