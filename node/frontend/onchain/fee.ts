// 金额始终使用整数分；费率与Rust费用政策的跨端金标共同验证。
import type { TransferItem } from './types';
declare const FLAT_FEE: number;
const MAX = (1n << 128n) - 1n;
export function parseYuanToFen(text: string): string {
    const clean = text.replace(/,/g, '').trim();
    if (!/^(?:0|[1-9][0-9]*)(?:\.[0-9]{1,2})?$/.test(clean))
        throw Error('金额最多保留两位小数');
    const [whole, decimal = ''] = clean.split('.');
    if(whole.length>37)throw Error('金额超出允许范围');
    const fen = BigInt(whole) * 100n + BigInt(decimal.padEnd(2, '0'));
    if (fen <= 0n || fen > MAX)
        throw Error('金额超出允许范围');
    return fen.toString();
}
export function formatFen(text: string | bigint): string {
    const value = BigInt(text);
    return `${(value / 100n).toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',')}.${(value % 100n).toString().padStart(2, '0')}`;
}
export function transferFeeFen(amount: string, remark = ''): bigint {
    const fen = BigInt(amount);
    if (fen <= 0n || fen > MAX)
        throw Error('金额超出允许范围');
    const base = (fen + 500n) / 1000n;
    return (base < 10n ? 10n : base) + (remark.length ? BigInt(FLAT_FEE) : 0n);
}
export function quoteTransfers(items: TransferItem[]) {
    let amount = 0n, fee = 0n;
    for (const item of items) {
        if (!/^[1-9][0-9]*$/.test(item.amountFen))
            throw Error('金额必须为正整数分');
        amount += BigInt(item.amountFen);
        fee += transferFeeFen(item.amountFen, item.remark);
        if (amount > MAX || fee > MAX || amount + fee > MAX)
            throw Error('转账合计超出允许范围');
    }
    return { amount, fee, total: amount + fee };
}
