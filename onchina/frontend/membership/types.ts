// 平台会员价格模块 DTO。金额一律使用十进制字符串，避免 JavaScript 数字精度损失。

export type PlatformMembershipLevel = 'freedom' | 'democracy' | 'spark';

export type PlatformPrices = {
  platform_cid_number: string;
  freedom_price_fen: string;
  democracy_price_fen: string;
  spark_price_fen: string;
  finalized_block_hash: string;
};

export type ProposePlatformPriceResult = {
  request_id: string;
  sign_request: string;
};
