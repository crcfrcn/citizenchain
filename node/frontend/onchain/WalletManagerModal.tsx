import { accountIdToSs58 } from "../client/ss58";
import { sanitizeError } from "../client/tauri";
import { AddressScanModal } from "../desktop/AddressScanModal";
import { transactionApi as api } from "./api";
import type { SignMode, Wallet, WalletStore, WalletBalanceState } from "./types";

import scanIcon from "../../../icons/scan.svg?url";
import { useState, useEffect } from 'react';

type Props = {
  wallets: Wallet[];
  activeAccountId: string | null;
  onClose: () => void;
  onUpdate: (store: WalletStore) => void;
  /** 选中钱包后回调，传回该账户的余额状态。 */
  onSelect: (wallet: Wallet, balance: WalletBalanceState) => void;
};

function signModeLabel(signMode: SignMode): string {
  switch (signMode) {
    case 'hot':
      return 'Hot';
    case 'cold':
      return 'Cold';
    default:
      throw new Error(`不支持的 signMode: ${String(signMode)}`);
  }
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

export function WalletManagerModal({ wallets, activeAccountId, onClose, onUpdate, onSelect }: Props) {
  const [name, setName] = useState('');
  const [address, setAddress] = useState('');
  const [adding, setAdding] = useState(false);
  const [showScan, setShowScan] = useState(false);
  const [confirmDeleteAccountId, setConfirmDeleteAccountId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // 每个钱包的余额缓存以唯一 account_id 为键。
  const [balances, setBalances] = useState<Record<string, WalletBalanceState>>({});

  // 加载所有钱包余额
  useEffect(() => {
    let cancelled = false;
    setBalances({});
    for (const w of wallets) {
      api.getWalletBalance(w.accountId)
        .then((b) => {
          if (!cancelled) setBalances((prev) => ({ ...prev, [w.accountId]: { status: 'ready', balance: b ?? '0' } }));
        })
        .catch((e) => {
          if (!cancelled) setBalances((prev) => ({ ...prev, [w.accountId]: { status: 'error', message: sanitizeError(e) } }));
        });
    }
    return () => { cancelled = true; };
  }, [wallets]);

  const handleSelectWallet = async (wallet: Wallet) => {
    try {
      const store = await api.setActiveWallet(wallet.accountId);
      onUpdate(store);
      onSelect(wallet, balances[wallet.accountId] ?? { status: 'loading' });
      onClose();
    } catch (e) {
      setError(sanitizeError(e));
    }
  };

  const handleDeleteClick = async (accountId: string) => {
    if (confirmDeleteAccountId === accountId) {
      try {
        const store = await api.removeWallet(accountId);
        onUpdate(store);
        setConfirmDeleteAccountId(null);
      } catch (e) {
        setError(sanitizeError(e));
      }
    } else {
      setConfirmDeleteAccountId(accountId);
    }
  };

  const handleAdd = async () => {
    if (!name.trim() || !address.trim()) return;
    setAdding(true);
    setError(null);
    try {
      await api.addWallet(name.trim(), address.trim());
      const store = await api.getWallets();
      onUpdate(store);
      setName('');
      setAddress('');
    } catch (e) {
      setError(sanitizeError(e));
    } finally {
      setAdding(false);
    }
  };

  return (
    <div className="wallet-manager-modal-mask" onClick={onClose}>
      <div className="wallet-manager-modal" onClick={(e) => e.stopPropagation()}>
        <h3>
          钱包管理
          <span className="wallet-manager-close" onClick={onClose}>&times;</span>
        </h3>

        {error && <div className="error">{error}</div>}

        {/* 添加区（上方） */}
        <div className="wallet-add-section-top">
          <input
            type="text"
            placeholder="钱包名称"
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="wallet-add-input"
          />
          <div className="address-input-row">
            <input
              type="text"
              placeholder="SS58 地址"
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              className="wallet-add-input"
            />
            <button type="button" className="scan-icon-btn" onClick={() => setShowScan(true)} title="扫码填入">
              <span aria-hidden="true" style={{display:"inline-block",width:18,height:18,backgroundColor:"currentColor",maskImage:`url("${scanIcon}")`,WebkitMaskImage:`url("${scanIcon}")`,maskSize:"contain",WebkitMaskSize:"contain",maskRepeat:"no-repeat",WebkitMaskRepeat:"no-repeat"}} />
            </button>
          </div>
          {showScan && (
            <AddressScanModal
              onResult={(result) => { setAddress(result.ss58Address); setShowScan(false); }}
              onClose={() => setShowScan(false)}
            />
          )}
          <button
            className="wallet-add-btn-half"
            disabled={adding || !name.trim() || !address.trim()}
            onClick={handleAdd}
          >
            {adding ? '添加中...' : '添加冷钱包'}
          </button>
        </div>

        {/* 分割线 */}
        <hr className="wallet-divider" />

        {/* 钱包列表（下方，可滚动） */}
        <div className="wallet-list-scroll">
          {wallets.length === 0 && (
            <p style={{ color: 'var(--text-muted)', textAlign: 'center' }}>暂无钱包</p>
          )}
          {wallets.map((w) => {
            const bal = balances[w.accountId];
            return (
              <div
                key={w.accountId}
                className={`wallet-row ${w.accountId === activeAccountId ? 'active' : ''}`}
                onClick={() => handleSelectWallet(w)}
              >
                <input
                  type="radio"
                  checked={w.accountId === activeAccountId}
                  readOnly
                  className="wallet-radio"
                />
                <span className="wallet-row-name">{w.name} · {signModeLabel(w.signMode)}</span>
                <span className="wallet-row-address">{accountIdToSs58(w.accountId)}</span>
                <span className="wallet-row-balance" title={bal?.status === 'error' ? bal.message : undefined}>
                  {bal?.status === 'ready' ? `${fenToYuan(bal.balance)} 元`
                    : bal?.status === 'error' ? '查询失败' : '查询中...'}
                </span>
                {w.signMode === 'cold' && (
                  <button
                    className={`wallet-row-delete-text ${confirmDeleteAccountId === w.accountId ? 'confirm' : ''}`}
                    onClick={(e) => { e.stopPropagation(); handleDeleteClick(w.accountId); }}
                  >
                    {confirmDeleteAccountId === w.accountId ? '确认' : '删除'}
                  </button>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
