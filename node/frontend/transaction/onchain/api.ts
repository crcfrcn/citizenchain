import { invoke } from '../../tauri';
import type { TransferSignRequestResult, TransferSubmitResult, Wallet, WalletStore } from './types';

// 首页交易面板专用 Tauri API；账户唯一键统一使用 account_id。
export const transactionApi = {
  getWallets: () => invoke<WalletStore>('get_wallets'),
  addWallet: (name: string, ss58Address: string) =>
    invoke<Wallet>('add_wallet', { name, ss58_address: ss58Address }),
  removeWallet: (accountId: string) =>
    invoke<WalletStore>('remove_wallet', { account_id: accountId }),
  setActiveWallet: (accountId: string) =>
    invoke<WalletStore>('set_active_wallet', { account_id: accountId }),
  getWalletBalance: (accountId: string) =>
    invoke<string | null>('get_wallet_balance', { account_id: accountId }),
  buildColdTransferRequest: (accountId: string, toSs58Address: string, amountYuan: number, remark: string) =>
    invoke<TransferSignRequestResult>('build_cold_transfer_request', {
      account_id: accountId,
      to_ss58_address: toSs58Address,
      amount_yuan: amountYuan,
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
      account_id: accountId,
      to_ss58_address: toSs58Address,
      amount_yuan: amountYuan,
      remark,
      unlock_password: unlockPassword,
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
      account_id: accountId,
      request_id: requestId,
      expected_payload_hash: expectedPayloadHash,
      call_data_hex: callDataHex,
      sign_nonce: signNonce,
      sign_block_number: signBlockNumber,
      response_json: responseJson,
    }),
};
