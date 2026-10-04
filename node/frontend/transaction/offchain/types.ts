// 清算行 offchain 网络 DTO 与页面状态机类型。
//
//
// - 机构身份只读 DTO 收敛在同目录 institution/types.ts(供清算行结算直读链上机构事实)。
// - 本文件只保留清算行节点声明、连通性检测、管理员解锁和 offchain 入口状态机。

import type { InstitutionAdminInfo } from '../../governance/types';
import type { AccountWithBalance } from './institution/types';

export type {
  AccountWithBalance,
  EligibleClearingBankCandidate,
  InstitutionDetail,
} from './institution/types';

export type ClearingBankNodeOnChainInfo = {
  cidNumber: string;
  peerId: string;
  rpcDomain: string;
  rpcPort: number;
  registeredAt: number;
  registered_by_account_id: string;
  registered_by_ss58_address: string;
};

export type ConnectivityCheck = {
  label: string;
  ok: boolean;
  detail?: string | null;
};

export type ConnectivityTestReport = {
  allOk: boolean;
  checks: ConnectivityCheck[];
};

export type DecryptedAdminInfo = {
  signer_public_key: string;
  cidNumber: string;
  decryptedAtMs: number;
};

export type DecryptAdminRequestResult = {
  requestJson: string;
  requestId: string;
  expectedPayloadHash: string;
  payloadHex: string;
};

/**
 * offchain/section.tsx 状态机。
 *
 *   empty                        初始 — 顶部 +添加清算行 按钮
 *   add-input-cid               输入 cid_number 或机构名,debounce 自动搜 CID 候选
 *   check-multisig               链上查 Institutions[cid_number]
 *                                  ├─ 已存在 -> institution-detail
 *                                  └─ 不存在 -> 提示去 onchina 控制台创建机构(节点不承接)
 *   institution-detail           机构详情卡片栅格 + 折叠子页入口 + 节点信息内联
 *   other-accounts-list          其他账户列表子页(折叠卡片入口)
 *   admin-list                   管理员列表子页(折叠卡片入口)
 *   declare-node                 多签 Active 但未声明节点 -> 填 RPC + 自测 + 签名声明
 */
export type ClearingBankView =
  | { kind: 'empty' }
  | { kind: 'add-input-cid' }
  | { kind: 'check-multisig'; cidNumber: string; cidFullName: string }
  | { kind: 'institution-detail'; cidNumber: string }
  | { kind: 'declare-node'; cidNumber: string; cidFullName: string }
  | { kind: 'other-accounts-list'; cidNumber: string; otherAccounts: AccountWithBalance[] }
  | { kind: 'admin-list'; cidNumber: string; admins: InstitutionAdminInfo[]; threshold: number; adminsLen: number };
