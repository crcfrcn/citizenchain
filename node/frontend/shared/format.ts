/**
 * 将链上余额（分）格式化为带千分位的元显示。
 * 1 元 = 100 分。
 */
export function formatBalance(fenStr: string): string {
  const fen = BigInt(fenStr);
  const negative = fen < 0n;
  const abs = negative ? -fen : fen;
  const yuan = abs / 100n;
  const remainder = abs % 100n;
  const yuanFormatted = yuan.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  const decimal = remainder.toString().padStart(2, '0');
  return `${negative ? '-' : ''}${yuanFormatted}.${decimal} 元`;
}

/** 金额千分位格式化工具。 */
export function formatAmount(value: string | null | undefined): string | null {
  if (value == null) return null;
  const trimmed = value.trim();
  if (!trimmed) return null;

  const match = trimmed.match(/^(-?[\d.]+)(.*)$/);
  if (!match) return trimmed;

  const [, numPart, suffix] = match;
  const [intPart, decimal] = numPart.split('.');
  const formatted = intPart.replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  const decimalStr = decimal != null ? `.${decimal}` : '';
  return `${formatted}${decimalStr}${suffix}`;
}
