// 封装机构提案分页、详情、计票和投票签名提交命令。

import { invoke } from "../client/tauri";
import type { GovernanceOverview, InstitutionDetail } from "../institution/council";
import type { FrozenVoteBinding, ProposalVoteSubmitResult, ProposalDisplayMeta, ProposalFullInfo, ProposalListItem, ProposalPageResult, UserVoteStatus } from "./types";
import type { VoteSignRequestResult } from "../client/signing";

export const proposalApi = {

  getProposalPage: (startId: string, count: number) =>
    invoke<ProposalPageResult>('get_proposal_page', { startId, count }),

  getProposalDetail: (proposalId: string) =>
    invoke<ProposalFullInfo>('get_proposal_detail', { proposalId }),

  getNextProposalId: () => invoke<string>('get_next_proposal_id'),

  getInstitutionProposals: (cidNumber: string) =>
    invoke<ProposalListItem[]>('get_institution_proposals', { cidNumber }),

  getInstitutionProposalPage: (cidNumber: string, startId: string, count: number) =>
    invoke<ProposalPageResult>('get_institution_proposal_page', { cidNumber, startId, count }),

  // 双层 ID + 反向索引
  getProposalDisplay: (proposalId: string) =>
    invoke<ProposalDisplayMeta | null>('get_proposal_display', { proposalId }),

  listProposalsByInstitutionCode: (institutionCode: string) =>
    invoke<string[]>('list_proposals_by_institution_code', { institutionCode }),

  listProposalsByCid: (cidNumber: string) =>
    invoke<string[]>('list_proposals_by_cid', { cidNumber }),

  listProposalsByOwner: (moduleTagHex: string) =>
    invoke<string[]>('list_proposals_by_owner', { moduleTagHex }),

  buildVoteRequest: (proposalId: string, signerPublicKey: string, voterRoleCode: string | null, approve: boolean) =>
    invoke<VoteSignRequestResult>('build_vote_request', {
      proposalId: proposalId,
      signerPublicKey,
      voterRoleCode: voterRoleCode,
      approve,
    }),

  buildJointVoteRequest: (proposalId: string, signerPublicKey: string, cidNumber: string, voterRoleCode: string, approve: boolean) =>
    invoke<VoteSignRequestResult>('build_joint_vote_request', {
      proposalId: proposalId,
      signerPublicKey,
      cidNumber: cidNumber,
      voterRoleCode: voterRoleCode,
      approve,
    }),

  submitVote: (
    requestId: string,
    expectedPublicKey: string,
    expectedPayloadHash: string,
    callDataHex: string,
    signNonce: number,
    signBlockNumber: number,
    responseJson: string,
    frozenTicket?: FrozenVoteBinding,
  ) =>
    invoke<ProposalVoteSubmitResult>('submit_vote', {
      requestId: requestId,
      expectedPublicKey,
      expectedPayloadHash: expectedPayloadHash,
      callDataHex: callDataHex,
      signNonce: signNonce,
      signBlockNumber: signBlockNumber,
      responseJson: responseJson,
      frozenTicket: frozenTicket ?? null,
    }),

  checkVoteStatus: (proposalId: string, signerPublicKey: string, cidNumber?: string, voterRoleCode?: string) =>
    invoke<UserVoteStatus>('check_vote_status', {
      proposalId: proposalId,
      signerPublicKey,
      cidNumber: cidNumber ?? null,
      voterRoleCode: voterRoleCode ?? null,
    })
};
