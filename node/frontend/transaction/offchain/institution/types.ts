import type { InstitutionAdminInfo } from '../../../governance/types';

// 清算行机构身份只读 DTO,与 Tauri 后端 transaction/offchain_transaction/institution_read/types.rs 对齐。

export type EligibleClearingBankCandidate = {
  cidNumber: string;
  cidFullName: string;
  refProperty: string;
  subType?: string | null;
  parentCidNumber?: string | null;
  parentCidFullName?: string | null;
  parentRefProperty?: string | null;
  provinceName: string;
  cityName: string;
  main_account_id?: string | null;
  fee_account_id?: string | null;
};

export type AccountWithBalance = {
  accountName: string;
  /** 唯一账户 ID（小写 0x + 64 位十六进制）。 */
  account_id: string;
  /** 仅用于展示的 SS58 地址（GMB prefix=2027）。 */
  ss58_address: string;
  /** `frame_system::Account[address].data.free`,最小单位"分"。 */
  balanceMinUnits: string;
  /** 友好元字符串 `xxx.xx`。 */
  balanceText: string;
  accountKind: 'main' | 'fee' | 'stake' | 'safety_fund' | 'he' | 'named';
  canClose: boolean;
};

export type InstitutionDetail = {
  cidNumber: string;
  cidFullName: string;
  /** 机构码（CID institution_code，[u8;4] 序列化为数字数组）。 */
  institutionCode: number[];

  main_account_info: AccountWithBalance;
  fee_account_info: AccountWithBalance;
  /** 主账户/费用账户之外的其它账户(自定义初始账户)。 */
  otherAccounts: AccountWithBalance[];

  adminsLen: number;
  threshold: number;
  /** 管理员账户及其全部有效岗位任职。 */
  admins: InstitutionAdminInfo[];

  createdAt: number;
  accountCount: number;
};

export type InstitutionProposalItem = {
  proposalId: number;
  kindLabel: string;
  statusLabel: string;
  summary: string;
};

export type InstitutionProposalPage = {
  items: InstitutionProposalItem[];
  hasMore: boolean;
};
