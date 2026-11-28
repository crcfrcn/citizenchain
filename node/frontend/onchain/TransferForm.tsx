// 单笔与批量共用当前钱包；分组仅用于输入，签名前展开为确定清单。
import { ADDRESS_INPUT_PLACEHOLDER, normalizeSs58Address } from '../client/ss58';
import { AddressScanModal } from '../desktop/scanner';
import { MetricHelp } from '../desktop/MetricHelp';
import { parseYuanToFen, formatFen, quoteTransfers } from './fee';
import type { Wallet, TransferDraft, TransferItem } from './types';
import citizenCoinLogo from '../../../icons/gmb.png?url';
import { useEffect, useRef, useState } from 'react';
type Props = {
    activeWallet: Wallet | null;
    batchEnabled?: boolean;
    nodeReady?: boolean;
    balance: string | null;
    balanceError?: string | null;
    submitError?: string | null;
    onRetryBalance?: () => void;
    onSubmit: (draft: TransferDraft) => void;
    disabled?: boolean;
    resetVersion?: number;
};
type Group = {
    id: number;
    amount: string;
    addresses: string;
};
function completeAddress(value: string) {
    if (/^0x[0-9a-fA-F]{40}$/.test(value))
        return true;
    try {
        normalizeSs58Address(value);
        return true;
    }
    catch {
        return false;
    }
}
function ScanIcon() { return <span aria-hidden="true" className="transfer-scan-icon"/>; }
export function TransferForm({ activeWallet, batchEnabled = true, nodeReady = true, balance, balanceError, submitError, onRetryBalance, onSubmit, disabled, resetVersion = 0 }: Props) {
    const [toAddress, setToAddress] = useState(''), [amount, setAmount] = useState(''), [remark, setRemark] = useState('');
    const [open, setOpen] = useState(false), [mode, setMode] = useState<'batch_all' | 'force_batch'>('batch_all');
    const [groups, setGroups] = useState<Group[]>([{ id: 0, amount: '', addresses: '' }]);
    const nextId = useRef(1);
    const dialogRef = useRef<HTMLElement>(null);
    useEffect(() => { if (open)
        dialogRef.current?.querySelector<HTMLInputElement>('input')?.focus(); }, [open]);
    const [deleting, setDeleting] = useState<number | null>(null), [scan, setScan] = useState<number | 'single' | null>(null), [error, setError] = useState<string | null>(null);
    const locked = Boolean(disabled || !activeWallet || !nodeReady);
    useEffect(() => { setAmount(''); setRemark(''); setOpen(false); setGroups([{ id: 0, amount: '', addresses: '' }]); setDeleting(null); setError(null); }, [resetVersion]);
    const patch = (id: number, change: Partial<Group>) => setGroups(prev => prev.map(g => g.id === id ? { ...g, ...change } : g));
    const single = (): TransferItem[] => [{ toAddress: toAddress.trim(), amountFen: parseYuanToFen(amount), remark }];
    const batch = (): TransferItem[] => groups.flatMap((group, groupIndex) => {
        const amountFen = parseYuanToFen(group.amount);
        const lines = group.addresses.split(/\r?\n/).map(line => line.trim()).filter(Boolean);
        if (!lines.length)
            throw Error(`第${groupIndex + 1}组缺少收款地址`);
        return lines.map((address, lineIndex) => {
            if (!completeAddress(address))
                throw Error(`第${groupIndex + 1}组第${lineIndex + 1}行地址无效`);
            return { toAddress: address, amountFen, remark: '' };
        });
    });
    let singleFee = '0', singleTotal = '0';
    try {
        const quote = quoteTransfers(single());
        singleFee = quote.fee.toString();
        singleTotal = quote.total.toString();
    }
    catch { }
    let batchQuote: {
        amount: bigint;
        fee: bigint;
        total: bigint;
    } | null = null, count = 0, batchError: string | null = null;
    try {
        const items = batch();
        count = items.length;
        batchQuote = quoteTransfers(items);
    }
    catch (e) {
        batchError = e instanceof Error ? e.message : String(e);
    }
    const submit = (isBatch: boolean) => {
        try {
            if (locked)
                return;
            const transfers = isBatch ? batch() : single();
            if (!isBatch && !completeAddress(toAddress.trim()))
                throw Error('收款地址无效');
            if (new TextEncoder().encode(remark).length > 99 && !isBatch)
                throw Error('转账备注不能超过99字节');
            quoteTransfers(transfers);
            setError(null);
            onSubmit({ mode: isBatch ? mode : 'transfer', transfers });
        }
        catch (e) {
            setError(e instanceof Error ? e.message : String(e));
        }
    };
    const amountInput = (value: string, change: (v: string) => void, label: string, placeholder = '请输入金额') => <input aria-label={label} type="text" inputMode="decimal" placeholder={placeholder} value={value} disabled={locked} onChange={e => change(e.target.value)}/>;
    return <div className="transfer-form">
    <div className="transfer-form-balance-row"><span className="transfer-form-balance-label">{activeWallet?.name ?? '-'}</span><span className="transfer-form-balance-value">{!activeWallet || !nodeReady ? '-' : balanceError ? '查询失败' : balance != null ? `${formatFen(balance)} 元` : '查询中...'}</span></div>
    {balanceError && <div className="error" role="alert">余额查询失败：{balanceError}{onRetryBalance && <button type="button" onClick={onRetryBalance}>重试查询</button>}</div>}
    {!activeWallet && <p className="transfer-form-status-hint">请先在钱包管理中添加钱包</p>}
    <div className="transfer-form-field"><div className="batch-transfer-label"><label>收款地址</label><button type="button" className="metric-help-button" title={batchEnabled ? undefined : '链上批量转账尚未启用'} disabled={locked || !batchEnabled} onClick={() => { setOpen(true); setError(null); }}>批量转账</button></div><div className="address-input-row"><input aria-label="收款地址" placeholder={ADDRESS_INPUT_PLACEHOLDER} value={toAddress} disabled={locked} onChange={e => setToAddress(e.target.value)}/><button type="button" className="scan-icon-btn" title="扫码填入" disabled={locked} onClick={() => setScan('single')}><ScanIcon /></button></div></div>
    <div className="transfer-form-field"><label>转账金额</label><div className="transfer-form-amount-row">{amountInput(amount, setAmount, '转账金额', '0.00')}<img className="transfer-form-currency" src={citizenCoinLogo} alt="公民币" title="公民币"/></div></div>
    <div className="transfer-form-field"><label htmlFor="transfer-form-remark">转账备注（选填）</label><textarea id="transfer-form-remark" className="transfer-form-remark" placeholder="输入备注收1.00元备注手续费。" value={remark} disabled={locked} onChange={e => setRemark(e.target.value)}/><span className="transfer-form-remark-counter">{new TextEncoder().encode(remark).length}/99 字节</span></div>
    <div className="transfer-form-summary"><div className="transfer-form-summary-line"><span>预计手续费</span><span>{formatFen(singleFee)} 元</span></div><div className="transfer-form-summary-line total"><span>合计</span><span>{formatFen(singleTotal)} 元</span></div></div>
    {(error || submitError) && !open && <div className="error" role="alert">{error ?? submitError}</div>}
    <button className="transfer-form-submit" disabled={locked || !toAddress.trim() || singleTotal === '0' || new TextEncoder().encode(remark).length > 99} onClick={() => submit(false)}>{disabled ? '签名中...' : '签名交易'}</button>
    {open && <div className="wallet-manager-modal-mask"><section ref={dialogRef} onKeyDown={event => {
                // 说明弹窗经Portal进入原生顶层，键盘行为交由该弹窗处理。
                if (!event.currentTarget.contains(event.target as Node))
                    return;
                if (event.key === 'Escape') {
                    event.preventDefault();
                    event.stopPropagation();
                }
                if (event.key !== 'Tab' || scan !== null)
                    return;
                const elements = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), textarea:not(:disabled)'));
                const first = elements[0], last = elements[elements.length - 1];
                if (event.shiftKey && document.activeElement === first) {
                    event.preventDefault();
                    last?.focus();
                }
                else if (!event.shiftKey && document.activeElement === last) {
                    event.preventDefault();
                    first?.focus();
                }
            }} className="wallet-manager-modal batch-transfer-modal" role="dialog" aria-modal="true" aria-labelledby="batch-transfer-title">
      <header className="wallet-manager-header"><button type="button" className="wallet-manager-add" disabled={locked} onClick={() => { setGroups(prev => [...prev, { id: nextId.current++, amount: '', addresses: '' }]); setDeleting(null); }}>添加</button><h3 id="batch-transfer-title">批量转账</h3><button type="button" className="wallet-manager-close" aria-label="关闭批量转账" disabled={Boolean(disabled)} onClick={() => setOpen(false)}>&times;</button></header>
      <div className="wallet-manager-body"><div className="batch-transfer-groups">{groups.map((g, i) => <div className="batch-transfer-group" key={g.id}>
        {i === 0 ? <span className="wallet-radio hot" aria-label="默认组不可删除"/> : <button type="button" className={`wallet-radio cold${deleting === g.id ? ' confirm' : ''}`} disabled={locked} aria-label={`${deleting === g.id ? '确认删除' : '删除'}第${i + 1}组`} onClick={() => { if (deleting === g.id) {
                setGroups(prev => prev.filter(v => v.id !== g.id));
                setDeleting(null);
            }
            else
                setDeleting(g.id); }}/>}
        {amountInput(g.amount, v => patch(g.id, { amount: v }), `第${i + 1}组金额`)}
        <textarea aria-label={`第${i + 1}组地址`} placeholder={ADDRESS_INPUT_PLACEHOLDER} rows={Math.max(1, g.addresses.split('\n').length)} wrap="off" value={g.addresses} disabled={locked} onChange={e => { let value = e.target.value; const last = value.split('\n').slice(-1)[0]?.trim() ?? ''; if (last && completeAddress(last))
                value += '\n'; patch(g.id, { addresses: value }); }}/>
        <button type="button" className="scan-icon-btn" title={`扫描第${i + 1}组地址`} disabled={locked} onClick={() => setScan(g.id)}><ScanIcon /></button>
      </div>)}</div></div>
      <div className="transfer-form-summary batch-transfer-summary">转账数量：{count} 笔｜转账金额：{formatFen(batchQuote?.amount ?? 0n)} 元｜预计手续费：{formatFen(batchQuote?.fee ?? 0n)} 元｜合计：{formatFen(batchQuote?.total ?? 0n)} 元</div>
      {(error || submitError || batchError) && groups.some(g => g.amount || g.addresses) && <div className="error" role="alert">{error ?? submitError ?? batchError}</div>}
      <footer className="batch-transfer-footer">
        <div className="batch-transfer-mode" role="group" aria-label="交易模式" aria-disabled={locked}><span className="batch-transfer-mode-title">交易模式</span><div className="batch-transfer-options"><label><input type="radio" name="batch-mode" disabled={locked} checked={mode === 'batch_all'} onChange={() => setMode('batch_all')}/>整批成功</label><label><input type="radio" name="batch-mode" disabled={locked} checked={mode === 'force_batch'} onChange={() => setMode('force_batch')}/>逐笔成功</label><MetricHelp title="交易模式说明" items={[
          '整批成功是指本批所有交易要么全部成功，有一笔失败则全部失败；',
          '逐笔成功是指本批所有交易按笔成功或失败，失败不影响成功的交易；',
          '成功的交易才会收取链上交易手续费，失败的则不收取链上交易手续费；',
          '可将地址添加到钱包管理中，添加后可在交易记录中查询该钱包所有交易。',
        ]}/></div></div>
        <button className="transfer-form-submit" disabled={locked || !batchQuote} onClick={() => submit(true)}>签名交易</button>
      </footer>
    </section></div>}
    {scan !== null && <AddressScanModal onResult={result => { if (scan === 'single')
        setToAddress(result.ss58Address);
    else {
        const group = groups.find(g => g.id === scan);
        if (group) {
            const prefix = group.addresses.trimEnd();
            patch(scan, { addresses: (prefix ? prefix + '\n' : '') + result.ss58Address + '\n' });
        }
    } setScan(null); }} onClose={() => setScan(null)}/>}
  </div>;
}
