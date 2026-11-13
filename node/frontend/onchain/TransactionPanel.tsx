import { sanitizeError } from "../client/tauri";
import { transactionApi as api } from "./api";
import type { TransferDraft, Wallet, WalletStore, WalletBalanceState } from "./types";
import { WalletManagerModal } from "./WalletManagerModal";
import { TransferForm } from "./TransferForm";
import { TransferSigningFlow } from "./TransferSigningFlow";
import { MetricHelp } from "../desktop/MetricHelp";
import { useState, useEffect, useRef, useCallback } from 'react';

export function TransactionPanel({ nodeReady }: { nodeReady: boolean }) {
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
  const balanceState = nodeReady && balanceResult?.accountId === activeWallet?.accountId
    ? balanceResult?.state : undefined;

  // 加载钱包列表
  useEffect(() => {
    api.getWallets().then((store) => {
      setWallets(store.wallets);
      setActiveAccountId(store.activeAccountId);
    }).catch((e) => setError(sanitizeError(e)));
  }, []);

  // RPC 就绪后立即查询当前钱包余额，随后每 5 秒刷新。
  useEffect(() => {
    if (!nodeReady || !activeWallet) { setBalanceResult(null); return; }
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
  }, [activeWallet?.accountId, balanceRetry, nodeReady]);

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
        <div className="transaction-panel-heading">
          <h2>转账</h2>
          <MetricHelp title="链上转账说明" items={[
            '链上转账交易是指将交易发送到完全去中心化的公民区块链网络进行的转账交易。',
            '链上转账交易支持公民链和以太坊，可在收款地址栏输入公民链账户或以太坊账户。',
            '转账支持矿工热钱包或冷钱包，可在钱包管理中添加更多冷钱包，冷钱包使用扫码签名。',
            '签名交易完成后，等待权威节点完成最终性确认，最终性确认后，转账交易将永久固化。',
            <>权威节点由1个国家储委会节点+43个省储委会节点组成，<strong>达30个节点验证即可形成最终性。</strong></>,
          ]} />
        </div>
        <button className="wallet-manage-btn" onClick={() => setShowWalletModal(true)}>
          钱包管理
        </button>
      </div>

      {error && <div className="error">{error}</div>}
      {successMsg && <div className="transfer-success-msg">{successMsg}</div>}

      <TransferForm
        activeWallet={activeWallet}
        nodeReady={nodeReady}
        balance={balanceState?.status === 'ready' ? balanceState.balance : null}
        balanceError={balanceState?.status === 'error' ? balanceState.message : null}
        onRetryBalance={() => setBalanceRetry((value) => value + 1)}
        onSubmit={handleTransferSubmit}
        disabled={signingFlow != null}
      />

      <div className="transfer-principle-note">
        <p>
          先有人类后有国家，是公民建立国家，国家是公民的国家，是公民治理国家，而不是国家统治公民。遵守公民宪法，加入公民链网络、使用公民币、做自由公民。
        </p>
      </div>

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
