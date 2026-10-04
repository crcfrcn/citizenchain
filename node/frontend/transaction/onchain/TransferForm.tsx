import { useState } from 'react';
import { AddressScanModal } from '../../shared/qr/AddressScanModal';
import { calculateTransferFeeYuan } from './fee';
import type { Wallet } from './types';

type Props = {
  activeWallet: Wallet | null;
  balance: string | null;
  onSubmit: (toAddress: string, amountYuan: number, remark: string) => void;
  disabled?: boolean;
};

const MAX_TRANSFER_REMARK_BYTES = 99;

/** 千分位格式化（元）：1234567.89 → "1,234,567.89" */
function fmtYuan(v: number): string {
  const fixed = v.toFixed(2);
  const [int, dec] = fixed.split('.');
  return `${int.replace(/\B(?=(\d{3})+(?!\d))/g, ',')}.${dec}`;
}

/** 分 → 千分位元 */
function fenToYuan(fenStr: string): string {
  const fen = BigInt(fenStr);
  const neg = fen < 0n;
  const abs = neg ? -fen : fen;
  const yuan = abs / 100n;
  const rem = abs % 100n;
  const yuanStr = yuan.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  const dec = rem.toString().padStart(2, '0');
  return `${neg ? '-' : ''}${yuanStr}.${dec}`;
}

/** 去掉逗号后解析数字 */
function parseAmount(s: string): number {
  const clean = s.replace(/,/g, '').trim();
  if (clean === '' || clean === '.') return 0;
  const v = parseFloat(clean);
  return isNaN(v) ? 0 : v;
}

/** 给纯数字字符串的整数部分加千分位，保留用户正在输入的小数部分 */
function addThousandSep(s: string): string {
  const clean = s.replace(/,/g, '');
  // 允许空、纯小数点等输入中间状态
  if (clean === '' || clean === '.') return clean;
  const dotIdx = clean.indexOf('.');
  const intPart = dotIdx >= 0 ? clean.slice(0, dotIdx) : clean;
  const decPart = dotIdx >= 0 ? clean.slice(dotIdx) : '';
  const formatted = intPart.replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  return formatted + decPart;
}

function utf8ByteLength(value: string): number {
  return new TextEncoder().encode(value).length;
}

export function TransferForm({ activeWallet, balance, onSubmit, disabled }: Props) {
  const [toAddress, setToAddress] = useState('');
  // 金额用字符串存储，显示带千分位
  const [amountText, setAmountText] = useState('');
  const [remark, setRemark] = useState('');
  const [showScan, setShowScan] = useState(false);

  const amount = parseAmount(amountText);
  const fee = calculateTransferFeeYuan(amount);
  const total = amount + fee;
  const remarkBytes = utf8ByteLength(remark);
  const formDisabled = disabled || !activeWallet;
  const remarkTooLong = remarkBytes > MAX_TRANSFER_REMARK_BYTES;
  const canSubmit =
    amount > 0 && toAddress.trim().length > 0 && !remarkTooLong && !formDisabled;

  const handleAmountChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const raw = e.target.value;
    // 只允许数字、逗号、小数点
    const filtered = raw.replace(/[^0-9.,]/g, '');
    setAmountText(addThousandSep(filtered));
  };

  const handleSubmit = () => {
    if (!canSubmit) return;
    onSubmit(toAddress.trim(), amount, remark);
  };

  return (
    <div className="transfer-form">
      {/* 余额 */}
      <div className="transfer-form-balance-row">
        <span className="transfer-form-balance-label">钱包可用余额</span>
        <span className="transfer-form-balance-value">
          {activeWallet && balance != null
            ? `${fenToYuan(balance)} 元`
            : (activeWallet ? '查询中...' : '-')
          }
        </span>
      </div>

      {!activeWallet && (
        <p className="transfer-form-status-hint">请先在钱包管理中添加钱包</p>
      )}

      {/* 收款地址 */}
      <div className="transfer-form-field">
        <label>收款地址</label>
        <div className="address-input-row">
          <input
            type="text"
            value={toAddress}
            onChange={(e) => setToAddress(e.target.value)}
            placeholder="请输入收款账户"
            disabled={formDisabled}
          />
          <button type="button" className="scan-icon-btn" onClick={() => setShowScan(true)} disabled={formDisabled} title="扫码填入">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M3 7V5a2 2 0 0 1 2-2h2"/><path d="M17 3h2a2 2 0 0 1 2 2v2"/><path d="M21 17v2a2 2 0 0 1-2 2h-2"/><path d="M7 21H5a2 2 0 0 1-2-2v-2"/>
              <rect x="7" y="7" width="10" height="10" rx="1"/>
            </svg>
          </button>
        </div>
      </div>

      {showScan && (
        <AddressScanModal
          onResult={(result) => {
            // 账户码只声明账户，不带金额；金额一律由本端手工填写。
            setToAddress(result.ss58_address);
            setShowScan(false);
          }}
          onClose={() => setShowScan(false)}
        />
      )}

      {/* 转账金额 — text 输入，千分位实时格式化 */}
      <div className="transfer-form-field">
        <label>转账金额</label>
        <div className="transfer-form-amount-row">
          <input
            type="text"
            inputMode="decimal"
            value={amountText}
            onChange={handleAmountChange}
            placeholder="0.00"
            disabled={formDisabled}
          />
          <span className="transfer-form-currency">GMB</span>
        </div>
      </div>

      <div className="transfer-form-field">
        <label>转账备注</label>
        <input
          type="text"
          value={remark}
          onChange={(e) => setRemark(e.target.value)}
          placeholder="选填，最多 99 字节"
          disabled={formDisabled}
        />
        <span className={`transfer-form-remark-counter${remarkTooLong ? ' is-error' : ''}`}>
          {remarkBytes}/{MAX_TRANSFER_REMARK_BYTES} 字节
        </span>
      </div>

      {/* 手续费 & 合计 */}
      <div className="transfer-form-summary">
        <div className="transfer-form-summary-line">
          <span>手续费</span>
          <span>{amount > 0 ? fmtYuan(fee) : '0.00'} 元</span>
        </div>
        <div className="transfer-form-summary-line total">
          <span>合计</span>
          <span>{amount > 0 ? fmtYuan(total) : '0.00'} 元</span>
        </div>
      </div>

      <button className="transfer-form-submit" disabled={!canSubmit} onClick={handleSubmit}>
        {disabled ? '签名中...' : '签名交易'}
      </button>
    </div>
  );
}
