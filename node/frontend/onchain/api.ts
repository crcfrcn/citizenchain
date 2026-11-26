// 调用钱包目录和转账准备、提交及余额原生命令。
import { invoke } from "../client/tauri";
import type { TransferSignRequestResult, TransferSubmitResult, Wallet, WalletStore, TransferMode, TransferItem, TransferResult } from "./types";
// 首页交易面板专用 Tauri API；账户唯一键统一使用 account_id。
export const transactionApi = {
  getWallets: () => invoke<WalletStore>('get_wallets'),
    addWallet: (name: string, ss58Address: string) => invoke<Wallet>('add_wallet', { name, ss58Address: ss58Address }),
    removeWallet: (accountId: string) => invoke<WalletStore>('remove_wallet', { accountId: accountId }),
    setActiveWallet: (accountId: string) => invoke<WalletStore>('set_active_wallet', { accountId: accountId }),
    getWalletBalance: (accountId: string) => invoke<string | null>('get_wallet_balance', { accountId: accountId }),
    prepareTransfer: (accountId: string, mode: TransferMode, transfers: TransferItem[]) => invoke<{
        transfers: TransferItem[];
        feeFen: string;
    }>('prepare_transfer', { accountId, mode, transfers }),
    buildColdTransferRequest: (accountId: string, mode: TransferMode, transfers: TransferItem[]) => invoke<TransferSignRequestResult>('build_cold_transfer_request', { accountId, mode, transfers }),
    submitHotTransfer: (accountId: string, mode: TransferMode, transfers: TransferItem[], unlockPassword: string) => invoke<TransferSubmitResult>('submit_hot_transfer', { accountId, mode, transfers, unlockPassword }),
    getTransferPolicy: () => invoke<{
        executionFee: boolean;
        batchEnabled: boolean;
    }>('get_transfer_policy'),
    getTransferResult: (txHash: string, cursor: number) => invoke<TransferResult>('get_transfer_result', { txHash, cursor }),
    submitColdTransfer: (accountId: string, requestId: string, expectedPayloadHash: string, callDataHex: string, signNonce: number, signBlockNumber: number, responseJson: string) => invoke<TransferSubmitResult>('submit_cold_transfer', {
      accountId: accountId,
      requestId: requestId,
      expectedPayloadHash: expectedPayloadHash,
      callDataHex: callDataHex,
      signNonce: signNonce,
      signBlockNumber: signBlockNumber,
      responseJson: responseJson,
    }),
};
