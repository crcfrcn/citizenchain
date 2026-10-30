import { invoke } from "../client/tauri";
import type { ProposalDisplayMeta, ProposalFullInfo, ProposalListItem, ProposalPageResult, UserVoteStatus } from "../proposal/types";
import type { VoteSignRequestResult, VoteSubmitResult } from "../client/signing";

export const councilApi = {

  getGovernanceOverview: () => invoke<GovernanceOverview>('get_governance_overview'),

  getInstitutionDetail: (cidNumber: string) =>
    invoke<InstitutionDetail>('get_institution_detail', { cidNumber }),

  startGovernanceBalanceWatch: (cidNumber: string) =>
    invoke<void>('start_governance_balance_watch', { cidNumber }),

  stopGovernanceBalanceWatch: (cidNumber: string) =>
    invoke<void>('stop_governance_balance_watch', { cidNumber })
};



// ── 管理员匹配（提案发起/投票签名流程共用） ──

export type AdminSignerMatch = {
  accountId: string;
  accountLabel: string;
  /// 当前机构内可供选择的任职岗位；同一账户可拥有多个岗位票据。
  roleAssignments?: InstitutionRoleAssignmentInfo[];
};


// ── 管理员激活 ──

export type ActivatedAdmin = {
  accountId: string;
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
  fullNameEn: string;
  shortNameEn: string;
  cidNumber: string;
  orgType: number;
  orgTypeLabel: string;
  mainAccountId: string;
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
  accountId: string;
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
  fullNameEn: string;
  shortNameEn: string;
  cidNumber: string;
  orgType: number;
  orgTypeLabel: string;
  mainAccountId: string;
  balanceFen: string | null;
  admins: AdminInfo[];
  internalThreshold: number;
  jointVoteWeight: number;
  stakeAccountId: string | null;
  stakingBalanceFen: string | null;
  feeAccountId: string | null;
  feeBalanceFen: string | null;
  cbAccountId: string | null;
  cbFeeFen: string | null;
  nrcAccountId: string | null;
  nrcFeeFen: string | null;
  fundAccountId: string | null;
  fundBalanceFen: string | null;
  warning: string | null;
};


export type InstitutionBalanceUpdate = {
  cidNumber: string;
  balanceFen: string | null;
  stakingBalanceFen: string | null;
  feeBalanceFen: string | null;
  cbFeeFen: string | null;
  nrcFeeFen: string | null;
  fundBalanceFen: string | null;
  warning: string | null;
};