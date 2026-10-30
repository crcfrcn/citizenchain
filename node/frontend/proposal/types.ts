import type { RuntimeUpgradeDetail } from '../upgrade/types';
import type { ResolutionIssuanceDetail } from './resolutionissue';
import type { ResolutionDestroyDetail } from './destroy';
import type { MultisigTransferProposalDetails } from "../multisig/types";



// ── 投票状态 ──

export type UserVoteStatus = {
  proposalId: number;
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
  seqInYear: number;
};


export type ProposalListItem = {
  proposalId: number;
  displayId: string;
  kind: number;
  kindLabel: string;
  stage: number;
  stageLabel: string;
  status: number;
  statusLabel: string;
  cidFullName: string | null;
  summary: string;
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
  proposalId: number;
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
  voterRoleSubjects: Array<{ cidNumber: string; roleCode: string }>;
  cidFullName: string | null;
};