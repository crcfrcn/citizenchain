// 交易模块类型定义。

/** 钱包账户签名模式闭集；任何运行时未知值都必须拒绝。 */
export type SignMode = 'hot' | 'cold';

export type Wallet = {
  name: string;
  /** hot 使用本机 powr 私钥；cold 使用 CitizenWallet 离线扫码。 */
  signMode: SignMode;
  /** 仅用于钱包界面展示的 SS58 地址（prefix 2027）。 */
  ss58Address: string;
  /** 账户 ID（小写 0x + 64 位十六进制）。 */
  accountId: string;
  createdAt: number;
};

export type WalletStore = {
  wallets: Wallet[];
  activeAccountId: string | null;
};

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
