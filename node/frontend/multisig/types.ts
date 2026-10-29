export type { AdminSignerMatch } from '../governance/types';

export type VoteSignRequestResult = {
  requestJson: string;
  requestId: string;
  expectedPayloadHash: string;
  signNonce: number;
  signBlockNumber: number;
};

export type VoteSubmitResult = {
  txHash: string;
};

export type TransferProposalDetail = {
  proposalId: number;
  actorCidNumber: string | null;
  fundingAccountId: string;
  beneficiaryAccountId: string;
  amountFen: string;
  remark: string;
  proposerAccountId: string;
};

export type SweepProposalDetail = {
  proposalId: number;
  actorCidNumber: string;
  institutionAccountId: string;
  amountFen: string;
};

export type SafetyFundProposalDetail = {
  proposalId: number;
  actorCidNumber: string;
  institutionAccountId: string;
  beneficiaryAccountId: string;
  amountFen: string;
  remark: string;
};

export type MultisigTransferProposalDetails = {
  transferDetail: TransferProposalDetail | null;
  safetyFundDetail: SafetyFundProposalDetail | null;
  sweepDetail: SweepProposalDetail | null;
};
