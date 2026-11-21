
// 与 charge_details 同口径；FLAT_FEE 在构建期从唯一 Rust 定义注入，单位为分。
declare const FLAT_FEE: number;

const ONCHAIN_MIN_FEE_FEN = 10;
const ONCHAIN_FEE_RATE = 0.001;

export function calculateTransferFeeYuan(amountYuan: number, remark = ''): number {
  const amountFen = Math.round(amountYuan * 100);
  if (amountFen <= 0) return 0;
  const feeFen = Math.max(Math.round(amountFen * ONCHAIN_FEE_RATE), ONCHAIN_MIN_FEE_FEN);
  return (feeFen + (remark.length > 0 ? FLAT_FEE : 0)) / 100;
}
