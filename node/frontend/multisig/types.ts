export type AdminSignerMatch = {
  ss58_address: string;
  account_id: string;
  account_label: string;
};

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
  funding_account_id: string;
  beneficiary_account_id: string;
  amountFen: string;
  remark: string;
  proposer_account_id: string;
};

export type SweepProposalDetail = {
  proposalId: number;
  actorCidNumber: string;
  institution_account_id: string;
  amountFen: string;
};

export type SafetyFundProposalDetail = {
  proposalId: number;
  actorCidNumber: string;
  institution_account_id: string;
  beneficiary_account_id: string;
  amountFen: string;
  remark: string;
};

export type MultisigTransferProposalDetails = {
  transferDetail: TransferProposalDetail | null;
  safetyFundDetail: SafetyFundProposalDetail | null;
  sweepDetail: SweepProposalDetail | null;
};
