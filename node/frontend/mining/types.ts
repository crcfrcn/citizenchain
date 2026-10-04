// 挖矿 tab 类型，对齐后端 src/mining/dashboard 与 src/mining/network_overview。

export type MiningIncome = {
  totalIncome: string;
  totalFeeIncome: string;
  totalRewardIncome: string;
  todayIncome: string;
};

export type MiningBlockRecord = {
  blockHeight: number;
  timestampMs: number | null;
  fee: string;
  blockReward: string;
  author: string;
};

export type MiningDashboard = {
  income: MiningIncome;
  records: MiningBlockRecord[];
  warning: string | null;
};

export type NetworkOverview = {
  onlineNodes: number;
  nrcNodes: number;
  prcNodes: number;
  prbNodes: number;
  fullNodes: number;
  lightNodes: number;
  warning: string | null;
};
