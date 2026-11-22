// 定义运行时升级提案公开详情。

export type RuntimeUpgradeDetail = {
  proposalId: string;
  proposerAccountId: string;
  reason: string;
  codeHashHex: string;
};