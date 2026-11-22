// 定义钱包签名模式、公开账户及转账响应模型。

export type SignMode = 'hot' | 'cold';

export type Wallet = {
  name: string;
  /** hot 使用本机 powr 私钥；cold 使用 CitizenWallet 离线扫码。 */
  signMode: SignMode;
  /** 账户 ID（小写 0x + 64 位十六进制）。 */
  accountId: string;
};

export type WalletStore = {
  wallets: Wallet[];
  activeAccountId: string | null;
};

export type WalletBalanceState =
  | { status: 'loading' }
  | { status: 'ready'; balance: string }
  | { status: 'error'; message: string };

export type TransferSignRequestResult = {
  requestJson: string;
  requestId: string;
  expectedPayloadHash: string;
  signNonce: number;
  signBlockNumber: number;
  callDataHex: string;
  feeYuan: number;
};

export type TransferSubmitResult = {
  txHash: string;
};

export type TransferDraft = {
  toAddress: string;
  amountYuan: number;
  remark: string;
};
