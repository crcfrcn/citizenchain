// 封装机构治理、本机岗位授权及余额订阅，并定义公开响应模型。

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



export type {RoleAccess,RoleSigner,RoleAccessRequest,InstitutionRoleInfo,InstitutionAssignmentInfo} from './assignments';
import type {InstitutionRoleInfo} from './assignments';

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
  roles: InstitutionRoleInfo[];
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
  heAccountId: string | null;
  heBalanceFen: string | null;
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
  heBalanceFen: string | null;
  warning: string | null;
};