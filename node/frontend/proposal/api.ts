import { invoke } from "../client/tauri";
import type { GovernanceOverview, InstitutionDetail } from "../institution/council";
import type { ProposalDisplayMeta, ProposalFullInfo, ProposalListItem, ProposalPageResult, UserVoteStatus } from "./types";
import type { VoteSignRequestResult, VoteSubmitResult } from "../client/signing";

export const proposalApi = {

  getProposalPage: (startId: number, count: number) =>
    invoke<ProposalPageResult>('get_proposal_page', { startId, count }),

  getProposalDetail: (proposalId: number) =>
    invoke<ProposalFullInfo>('get_proposal_detail', { proposalId }),

  getNextProposalId: () => invoke<number>('get_next_proposal_id'),

  getInstitutionProposals: (cidNumber: string) =>
    invoke<ProposalListItem[]>('get_institution_proposals', { cidNumber }),

  getInstitutionProposalPage: (cidNumber: string, startId: number, count: number) =>
    invoke<ProposalPageResult>('get_institution_proposal_page', { cidNumber, startId, count }),

  // 双层 ID + 反向索引
  getProposalDisplay: (proposalId: number) =>
    invoke<ProposalDisplayMeta | null>('get_proposal_display', { proposalId }),

  listProposalsByInstitutionCode: (institutionCode: string) =>
    invoke<number[]>('list_proposals_by_institution_code', { institutionCode }),

  listProposalsByCid: (cidNumber: string) =>
    invoke<number[]>('list_proposals_by_cid', { cidNumber }),

  listProposalsByOwner: (moduleTagHex: string) =>
    invoke<number[]>('list_proposals_by_owner', { moduleTagHex }),

  buildVoteRequest: (proposalId: number, signerPublicKey: string, voterRoleCode: string | null, approve: boolean) =>
    invoke<VoteSignRequestResult>('build_vote_request', {
      proposalId: proposalId,
      signerPublicKey,
      voterRoleCode: voterRoleCode,
      approve,
    }),

  buildJointVoteRequest: (proposalId: number, signerPublicKey: string, cidNumber: string, voterRoleCode: string, approve: boolean) =>
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
  ) =>
    invoke<VoteSubmitResult>('submit_vote', {
      requestId: requestId,
      expectedPublicKey,
      expectedPayloadHash: expectedPayloadHash,
      callDataHex: callDataHex,
      signNonce: signNonce,
      signBlockNumber: signBlockNumber,
      responseJson: responseJson,
    }),

  checkVoteStatus: (proposalId: number, signerPublicKey: string, cidNumber?: string, voterRoleCode?: string) =>
    invoke<UserVoteStatus>('check_vote_status', {
      proposalId: proposalId,
      signerPublicKey,
      cidNumber: cidNumber ?? null,
      voterRoleCode: voterRoleCode ?? null,
    })
};
