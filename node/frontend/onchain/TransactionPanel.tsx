import { sanitizeError } from "../client/tauri";
import { transactionApi as api } from "./api";
import type { TransferDraft, Wallet, WalletStore, WalletBalanceState } from "./types";
import { WalletManagerModal } from "./WalletManagerModal";
import { TransferForm } from "./TransferForm";
import { TransferSigningFlow } from "./TransferSigningFlow";
import { useState, useEffect, useRef, useCallback } from 'react';

export function TransactionPanel() {
  const [wallets, setWallets] = useState<Wallet[]>([]);
  const [activeAccountId, setActiveAccountId] = useState<string | null>(null);
  const [balanceResult, setBalanceResult] = useState<{
    accountId: string; state: WalletBalanceState;
  } | null>(null);
  const [balanceRetry, setBalanceRetry] = useState(0);
  const [showWalletModal, setShowWalletModal] = useState(false);
  const [signingFlow, setSigningFlow] = useState<TransferDraft | null>(null);
  const [successMsg, setSuccessMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const successTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const activeWallet = wallets.find((wallet) => wallet.accountId === activeAccountId) ?? null;
  const balanceState = balanceResult?.accountId === activeWallet?.accountId
    ? balanceResult?.state : undefined;

  // 加载钱包列表
  useEffect(() => {
    api.getWallets().then((store) => {
      setWallets(store.wallets);
      setActiveAccountId(store.activeAccountId);
    }).catch((e) => setError(sanitizeError(e)));
  }, []);

  // 轮询当前钱包余额（5s）
  useEffect(() => {
    if (!activeWallet) { setBalanceResult(null); return; }
    const accountId = activeWallet.accountId;
    let cancelled = false;
    let inFlight = false;
    setBalanceResult({ accountId, state: { status: 'loading' } });
    const fetch = async () => {
      if (inFlight) return;
      inFlight = true;
      try {
        const balance = await api.getWalletBalance(accountId);
        if (!cancelled) setBalanceResult({ accountId, state: { status: 'ready', balance: balance ?? '0' } });
      } catch (e) {
        if (!cancelled) setBalanceResult({ accountId, state: { status: 'error', message: sanitizeError(e) } });
      } finally {
        inFlight = false;
      }
    };
    fetch();
    const interval = setInterval(fetch, 5000);
    return () => { cancelled = true; clearInterval(interval); };
  }, [activeWallet?.accountId, balanceRetry]);

  const handleWalletUpdate = useCallback((store: WalletStore) => {
    setWallets(store.wallets);
    setActiveAccountId(store.activeAccountId);
  }, []);

  // 弹窗中选中钱包后，立即更新余额
  const handleWalletSelect = useCallback((wallet: Wallet, state: WalletBalanceState) => {
    setBalanceResult({ accountId: wallet.accountId, state });
  }, []);

  const handleTransferSubmit = useCallback((toAddress: string, amountYuan: number, remark: string) => {
    setSigningFlow({ toAddress, amountYuan, remark });
  }, []);

  const handleTransferSuccess = useCallback((txHash: string) => {
    setSigningFlow(null);
    // 此处只有交易池提交哈希，不能把提交成功显示为已上链成功。
    setSuccessMsg(`转账已提交，哈希: ${txHash.slice(0, 16)}...`);
    if (successTimer.current) clearTimeout(successTimer.current);
    successTimer.current = setTimeout(() => setSuccessMsg(null), 5000);
  }, []);

  return (
    <div className="transaction-panel">
      <div className="transaction-panel-header">
        <h2>交易</h2>
        <button className="wallet-manage-btn" onClick={() => setShowWalletModal(true)}>
          钱包管理
        </button>
      </div>

      {error && <div className="error">{error}</div>}
      {successMsg && <div className="transfer-success-msg">{successMsg}</div>}

      <TransferForm
        activeWallet={activeWallet}
        balance={balanceState?.status === 'ready' ? balanceState.balance : null}
        balanceError={balanceState?.status === 'error' ? balanceState.message : null}
        onRetryBalance={() => setBalanceRetry((value) => value + 1)}
        onSubmit={handleTransferSubmit}
        disabled={signingFlow != null}
      />

      {signingFlow && activeWallet && (
        <TransferSigningFlow
          wallet={activeWallet}
          toAddress={signingFlow.toAddress}
          amountYuan={signingFlow.amountYuan}
          remark={signingFlow.remark}
          onClose={() => setSigningFlow(null)}
          onSuccess={handleTransferSuccess}
        />
      )}

      {showWalletModal && (
        <WalletManagerModal
          wallets={wallets}
          activeAccountId={activeAccountId}
          onClose={() => setShowWalletModal(false)}
          onUpdate={handleWalletUpdate}
          onSelect={handleWalletSelect}
        />
      )}
    </div>
  );
}
