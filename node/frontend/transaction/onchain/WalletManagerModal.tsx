import { useState, useEffect } from 'react';
import { sanitizeError } from '../../tauri';
import { AddressScanModal } from '../../shared/qr/AddressScanModal';
import { transactionApi as api } from './api';
import type { SignMode, Wallet, WalletStore } from './types';

type Props = {
  wallets: Wallet[];
  activeAccountId: string | null;
  onClose: () => void;
  onUpdate: (store: WalletStore) => void;
  /** 选中钱包后回调，传回钱包和余额（分字符串） */
  onSelect: (wallet: Wallet, balanceFen: string | null) => void;
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
  const [balances, setBalances] = useState<Record<string, string | null>>({});

  // 加载所有钱包余额
  useEffect(() => {
    for (const w of wallets) {
      api.getWalletBalance(w.accountId)
        .then((b) => setBalances((prev) => ({ ...prev, [w.accountId]: b })))
        .catch(() => setBalances((prev) => ({ ...prev, [w.accountId]: null })));
    }
  }, [wallets]);

  const handleSelectWallet = async (wallet: Wallet) => {
    try {
      const store = await api.setActiveWallet(wallet.accountId);
      onUpdate(store);
      onSelect(wallet, balances[wallet.accountId] ?? null);
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
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M3 7V5a2 2 0 0 1 2-2h2"/><path d="M17 3h2a2 2 0 0 1 2 2v2"/><path d="M21 17v2a2 2 0 0 1-2 2h-2"/><path d="M7 21H5a2 2 0 0 1-2-2v-2"/>
                <rect x="7" y="7" width="10" height="10" rx="1"/>
              </svg>
            </button>
          </div>
          {showScan && (
            <AddressScanModal
              onResult={(result) => { setAddress(result.ss58_address); setShowScan(false); }}
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
                <span className="wallet-row-address">{w.ss58Address}</span>
                <span className="wallet-row-balance">
                  {bal != null ? `${fenToYuan(bal)} 元` : '-'}
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
