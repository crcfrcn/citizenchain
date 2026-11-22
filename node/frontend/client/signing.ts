// 定义投票签名准备和提交结果的前端模型。

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