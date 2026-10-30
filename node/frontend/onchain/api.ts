import { invoke } from "../client/tauri";
import type { TransferSignRequestResult, TransferSubmitResult, Wallet, WalletStore } from "./types";


// 首页交易面板专用 Tauri API；账户唯一键统一使用 account_id。
export const transactionApi = {
  getWallets: () => invoke<WalletStore>('get_wallets'),
  addWallet: (name: string, ss58Address: string) =>
    invoke<Wallet>('add_wallet', { name, ss58Address: ss58Address }),
  removeWallet: (accountId: string) =>
    invoke<WalletStore>('remove_wallet', { accountId: accountId }),
  setActiveWallet: (accountId: string) =>
    invoke<WalletStore>('set_active_wallet', { accountId: accountId }),
  getWalletBalance: (accountId: string) =>
    invoke<string | null>('get_wallet_balance', { accountId: accountId }),
  buildColdTransferRequest: (accountId: string, toSs58Address: string, amountYuan: number, remark: string) =>
    invoke<TransferSignRequestResult>('build_cold_transfer_request', {
      accountId: accountId,
      toSs58Address: toSs58Address,
      amountYuan: amountYuan,
      remark,
    }),
  submitHotTransfer: (
    accountId: string,
    toSs58Address: string,
    amountYuan: number,
    remark: string,
    unlockPassword: string,
  ) =>
    invoke<TransferSubmitResult>('submit_hot_transfer', {
      accountId: accountId,
      toSs58Address: toSs58Address,
      amountYuan: amountYuan,
      remark,
      unlockPassword: unlockPassword,
    }),
  submitColdTransfer: (
    accountId: string,
    requestId: string,
    expectedPayloadHash: string,
    callDataHex: string,
    signNonce: number,
    signBlockNumber: number,
    responseJson: string,
  ) =>
    invoke<TransferSubmitResult>('submit_cold_transfer', {
      accountId: accountId,
      requestId: requestId,
      expectedPayloadHash: expectedPayloadHash,
      callDataHex: callDataHex,
      signNonce: signNonce,
      signBlockNumber: signBlockNumber,
      responseJson: responseJson,
    }),
};
