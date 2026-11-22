// 导出多签提案使用的统一签名准备和提交类型。

export type { AdminSignerMatch } from '../institution/council';

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
  proposalId: string;
  actorCidNumber: string | null;
  fundingAccountId: string;
  beneficiaryAccountId: string;
  amountFen: string;
  remark: string;
  proposerAccountId: string;
};

export type SweepProposalDetail = {
  proposalId: string;
  actorCidNumber: string;
  institutionAccountId: string;
  amountFen: string;
};

export type SafetyFundProposalDetail = {
  proposalId: string;
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
