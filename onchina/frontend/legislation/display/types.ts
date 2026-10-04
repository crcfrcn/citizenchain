// 大屏看板前端类型,camelCase 逐字镜像后端 DTO
// (onchina/src/domains/legislation/display/model.rs)。复用 LegProposalState,不重定义。

import type { LegProposalState } from '../types';

/** 单个席位(议员)对当前提案的投票态(true 赞成 / false 反对 / null 未投)。 */
export interface SeatView {
  account_id: string;
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
