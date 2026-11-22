// 定义挖矿收入、出块记录及网络概览响应。

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
