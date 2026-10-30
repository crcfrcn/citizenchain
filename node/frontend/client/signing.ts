


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