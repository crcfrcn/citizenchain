import { invoke } from '../tauri';
import type {
  GovernanceOverview,
  InstitutionDetail,
  ProposalDisplayMeta,
  ProposalFullInfo,
  ProposalListItem,
  ProposalPageResult,
  UserVoteStatus,
  VoteSignRequestResult,
  VoteSubmitResult,
} from './types';

// 治理模块专用 Tauri API，对齐后端 src/governance。
export const governanceApi = {
  getGovernanceOverview: () => invoke<GovernanceOverview>('get_governance_overview'),
  getInstitutionDetail: (cidNumber: string) =>
    invoke<InstitutionDetail>('get_institution_detail', { cidNumber }),
  startGovernanceBalanceWatch: (cidNumber: string) =>
    invoke<void>('start_governance_balance_watch', { cidNumber }),
  stopGovernanceBalanceWatch: (cidNumber: string) =>
    invoke<void>('stop_governance_balance_watch', { cidNumber }),
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
  listProposalsByOwner: (moduleTagScaleHex: string) =>
    invoke<number[]>('list_proposals_by_owner', { moduleTagScaleHex }),
  buildVoteRequest: (proposalId: number, signer_public_key: string, voterRoleCode: string | null, approve: boolean) =>
    invoke<VoteSignRequestResult>('build_vote_request', {
      proposal_id: proposalId,
      signer_public_key,
      voter_role_code: voterRoleCode,
      approve,
    }),
  buildJointVoteRequest: (proposalId: number, signer_public_key: string, cidNumber: string, voterRoleCode: string, approve: boolean) =>
    invoke<VoteSignRequestResult>('build_joint_vote_request', {
      proposal_id: proposalId,
      signer_public_key,
      cid_number: cidNumber,
      voter_role_code: voterRoleCode,
      approve,
    }),
  submitVote: (
    requestId: string,
    expected_signer_public_key: string,
    expectedPayloadHash: string,
    callDataHex: string,
    signNonce: number,
    signBlockNumber: number,
    responseJson: string,
  ) =>
    invoke<VoteSubmitResult>('submit_vote', {
      request_id: requestId,
      expected_signer_public_key,
      expected_payload_hash: expectedPayloadHash,
      call_data_hex: callDataHex,
      sign_nonce: signNonce,
      sign_block_number: signBlockNumber,
      response_json: responseJson,
    }),
  checkVoteStatus: (proposalId: number, signer_public_key: string, cidNumber?: string, voterRoleCode?: string) =>
    invoke<UserVoteStatus>('check_vote_status', {
      proposal_id: proposalId,
      signer_public_key,
      cid_number: cidNumber ?? null,
      voter_role_code: voterRoleCode ?? null,
    }),
};
