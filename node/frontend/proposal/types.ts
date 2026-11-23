// 定义提案元数据、展示号、计票及用户投票状态。

import type { RuntimeUpgradeDetail } from '../upgrade/types';
import type { ResolutionIssuanceDetail } from './resolutionissue';
import type { ResolutionDestroyDetail } from './destroy';
import type { MultisigTransferProposalDetails } from "../multisig/types";



// ── 投票状态 ──

export type UserVoteStatus = {
  proposalId: string;
  kind: number;
  stage: number;
  voterRoleCode: string | null;
  internalVote: boolean | null;
  jointVote: boolean | null;
};


// ── 提案相关类型 ──

/// 双层 ID v1:展示号反查值。主键 `proposalId` 与展示号解耦。
export type ProposalDisplayMeta = {
  year: number;
  seqInYear: string;
};


export type ProposalListItem = {
  proposalId: string;
  displayId: string;
  kind: number;
  kindLabel: string;
  stage: number;
  stageLabel: string;
  status: number;
  statusLabel: string;
  cidFullName: string | null;
  summary: string;
  card?: {
    actionLabel: string;
    amountFen: string | null;
    remark: string | null;
  } | null;
};


export type ProposalPageResult = {
  items: ProposalListItem[];
  hasMore: boolean;
  warning: string | null;
};


export type VoteTally = {
  yes: number;
  no: number;
};


export type ProposalMeta = {
  proposalId: string;
  kind: number;
  stage: number;
  status: number;
  internalCode: string | null;
  actorCidNumber: string | null;
  executionAccountId: string | null;
  subjectCidNumbers: string[];
};


export type ProposalFullInfo = MultisigTransferProposalDetails & {
  meta: ProposalMeta;
  resolutionIssuanceDetail: ResolutionIssuanceDetail | null;
  resolutionDestroyDetail: ResolutionDestroyDetail | null;
  runtimeUpgradeDetail: RuntimeUpgradeDetail | null;
  internalTally: VoteTally | null;
  jointTally: VoteTally | null;
  referendumTally: { yes: number; no: number } | null;
  frozenVoteView: FrozenVoteView | null;
  voterRoleSubjects: Array<{ cidNumber: string; roleCode: string }>;
  cidFullName: string | null;
};

// 原提案的完整票据身份；版本使用字符串，避免JS整数精度丢失。
export type FrozenVoteBinding = { genesisHash: string; proposalId: string; cidNumber: string; roleCode: string; accountId: string; walletRevision: string };
export type FrozenVoter = { accountId: string; walletRevision: string | null; vote: boolean | null; voteError: string | null };
export type FrozenRoleSnapshot = { cidNumber: string; roleCode: string; error: string | null; voters: FrozenVoter[] };
export type FrozenVoteView = { genesisHash: string; blockHash: string; blockNumber: string; endBlock: string; kind: number; stage: number; status: number; actorCidNumber: string | null; scopeError: string | null; roles: FrozenRoleSnapshot[] };
export type ProposalVoteSubmitResult = { txHash: string | null; observed: boolean };
