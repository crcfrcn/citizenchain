// 平台会员价格模块 DTO。金额一律使用十进制字符串，避免 JavaScript 数字精度损失。

export type PlatformMembershipLevel = 'freedom' | 'democracy' | 'spark';

export type PlatformPrices = {
  platformCidNumber: string;
  freedomPriceFen: string;
  democracyPriceFen: string;
  sparkPriceFen: string;
  finalizedBlockHash: string;
};

export type ProposePlatformPriceResult = {
  requestId: string;
  signRequest: string;
};
