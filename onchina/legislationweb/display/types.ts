// 定义与后端一致的立法大屏席位、提案和看板类型。

import type { LegProposalState } from '../proposals/types';

/** 单个席位(议员)对当前提案的投票态(true 赞成 / false 反对 / null 未投)。 */
export interface SeatView {
  accountId: string;
  ticketId: string;
  roleCode: string;
  name: string;
  title: string;
  vote: boolean | null;
}

/** 单个活跃提案的看板视图(进度投影 + 席位板 + 聚合计数)。 */
export interface ActiveProposalView {
  state: LegProposalState;
  seats: SeatView[];
  approvedCount: number;
  rejectedCount: number;
  pendingCount: number;
}

/** 大屏看板顶层快照(本节点机构 + 名册规模 + 活跃提案列表)。 */
export interface DisplayBoard {
  institutionCode: string;
  cidShortName?: string | null;
  scopeLabel: string;
  rosterTotal: number;
  activeProposals: ActiveProposalView[];
}
