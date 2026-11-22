// 按链端费率及构建注入的固定费用计算转账手续费。

declare const FLAT_FEE: number;

const ONCHAIN_MIN_FEE_FEN = 10;
const ONCHAIN_FEE_RATE = 0.001;

export function calculateTransferFeeYuan(amountYuan: number, remark = ''): number {
  const amountFen = Math.round(amountYuan * 100);
  if (amountFen <= 0) return 0;
  const feeFen = Math.max(Math.round(amountFen * ONCHAIN_FEE_RATE), ONCHAIN_MIN_FEE_FEN);
  return (feeFen + (remark.length > 0 ? FLAT_FEE : 0)) / 100;
}
