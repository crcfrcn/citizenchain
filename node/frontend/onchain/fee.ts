// 首页交易手续费预估：与 runtime onchain_transaction::calculate_onchain_fee 保持同一口径。

const ONCHAIN_MIN_FEE_FEN = 10;
const ONCHAIN_FEE_RATE = 0.001;

export function calculateTransferFeeYuan(amountYuan: number): number {
  const amountFen = Math.round(amountYuan * 100);
  if (amountFen <= 0) return 0;
  const feeFen = Math.max(Math.round(amountFen * ONCHAIN_FEE_RATE), ONCHAIN_MIN_FEE_FEN);
  return feeFen / 100;
}
