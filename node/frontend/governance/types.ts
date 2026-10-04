// 治理模块前端类型定义，与后端 governance::types 和 admins_change::activation 对应。
import type { MultisigTransferProposalDetails } from '../transaction/multisig/types';

// ── 签名请求/响应 ──

export type VoteSignRequestResult = {
  requestJson: string;
  callDataHex: string;
  requestId: string;
  expectedPayloadHash: string;
  signNonce: number;
  signBlockNumber: number;
};

export type VoteSubmitResult = {
  txHash: string;
};

// ── 投票状态 ──

export type UserVoteStatus = {
  proposalId: number;
  kind: number;
  stage: number;
  voterRoleCode: string | null;
  internalVote: boolean | null;
  jointVote: boolean | null;
};

// ── 管理员匹配（提案发起/投票签名流程共用） ──

export type AdminSignerMatch = {
  ss58_address: string;
  account_id: string;
  account_label: string;
  /// 当前机构内可供选择的任职岗位；同一账户可拥有多个岗位票据。
  roleAssignments?: InstitutionRoleAssignmentInfo[];
};

// ── 管理员激活 ──

export type ActivatedAdmin = {
  account_id: string;
  cidNumber: string;
  institutionCode: number[];
  kind: number;
  activatedAtMs: number;
};

export type ActivateRequestResult = {
  requestJson: string;
  requestId: string;
  expectedPayloadHash: string;
  payloadHex: string;
};

// ── 机构 ──

export type InstitutionListItem = {
  cidFullName: string;
  cidShortName: string;
  cidFullNameEn: string;
  cidShortNameEn: string;
  cidNumber: string;
  orgType: number;
  orgTypeLabel: string;
  main_account_id: string;
};

export type GovernanceOverview = {
  nationalCouncils: InstitutionListItem[];
  provincialCouncils: InstitutionListItem[];
  provincialBanks: InstitutionListItem[];
  warning: string | null;
};

export type InstitutionRoleAssignmentInfo = {
  roleCode: string;
  roleName: string;
  termRequired: boolean;
  termStart: number;
  termEnd: number;
  assignmentSource: number;
  assignmentSourceLabel: string;
  assignmentSourceRef: string;
};

export type InstitutionAdminInfo = {
  account_id: string;
  familyName: string;
  givenName: string;
  assignments: InstitutionRoleAssignmentInfo[];
};

export type AdminInfo = InstitutionAdminInfo & {
  balanceFen: string | null;
};

export type InstitutionDetail = {
  cidFullName: string;
  cidShortName: string;
  cidFullNameEn: string;
  cidShortNameEn: string;
  cidNumber: string;
  orgType: number;
  orgTypeLabel: string;
  main_account_id: string;
  balanceFen: string | null;
  admins: AdminInfo[];
  internalThreshold: number;
  jointVoteWeight: number;
  stake_account_id: string | null;
  stakingBalanceFen: string | null;
  fee_account_id: string | null;
  feeBalanceFen: string | null;
  cb_fee_account_id: string | null;
  cbFeeBalanceFen: string | null;
  nrc_fee_account_id: string | null;
  nrcFeeBalanceFen: string | null;
  safety_fund_account_id: string | null;
  safetyFundBalanceFen: string | null;
  warning: string | null;
};

export type InstitutionBalanceUpdate = {
  cidNumber: string;
  balanceFen: string | null;
  stakingBalanceFen: string | null;
  feeBalanceFen: string | null;
  cbFeeBalanceFen: string | null;
  nrcFeeBalanceFen: string | null;
  safetyFundBalanceFen: string | null;
  warning: string | null;
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
  execution_account_id: string | null;
  subjectCidNumbers: string[];
};

export type RuntimeUpgradeDetail = {
  proposalId: number;
  proposer_account_id: string;
  reason: string;
  codeHashHex: string;
};

export type ProposalFullInfo = MultisigTransferProposalDetails & {
  meta: ProposalMeta;
  runtimeUpgradeDetail: RuntimeUpgradeDetail | null;
  internalTally: VoteTally | null;
  jointTally: VoteTally | null;
  referendumTally: { yes: number; no: number } | null;
  voterRoleSubjects: Array<{ cidNumber: string; roleCode: string }>;
  cidFullName: string | null;
};
