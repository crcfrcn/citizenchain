// 组织钱包选择、余额及链上转账界面。
import { sanitizeError } from "../client/tauri";
import { transactionApi as api } from "./api";
import type { TransferDraft, Wallet, WalletStore, WalletBalanceState, TransferSubmitResult, TransferResult } from "./types";
import { WalletManagerModal } from "./WalletManagerModal";
import { TransferForm } from "./TransferForm";
import { TransferSigningFlow } from "./TransferSigningFlow";
import { formatFen, quoteTransfers } from './fee';
import { MetricHelp } from "../desktop/MetricHelp";
import { useState, useEffect, useRef, useCallback } from 'react';
export function TransactionPanel({ nodeReady }: {
    nodeReady: boolean;
}) {
    const preparingRef = useRef(false);
    const [preparing, setPreparing] = useState(false);
    const [policy, setPolicy] = useState({ executionFee: false, batchEnabled: false });
  const [wallets, setWallets] = useState<Wallet[]>([]);
  const [activeAccountId, setActiveAccountId] = useState<string | null>(null);
  const [balanceResult, setBalanceResult] = useState<{
        accountId: string;
        state: WalletBalanceState;
  } | null>(null);
  const [balanceRetry, setBalanceRetry] = useState(0);
  const [showWalletModal, setShowWalletModal] = useState(false);
    const [signingFlow, setSigningFlow] = useState<(TransferDraft & {
        wallet: Wallet;
        executionFee: boolean;
    }) | null>(null);
  const [successMsg, setSuccessMsg] = useState<string | null>(null);
  const [resetVersion, setResetVersion] = useState(0);
    const [pending, setPending] = useState<{
        hash: string;
        cursor: number;
        draft: TransferDraft;
        result?: TransferResult;
    }[]>([]);
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
        if (!nodeReady || !activeWallet) {
            setBalanceResult(null);
            return;
        }
    const accountId = activeWallet.accountId;
    let cancelled = false;
    let inFlight = false;
    setBalanceResult({ accountId, state: { status: 'loading' } });
    const fetch = async () => {
            if (inFlight)
                return;
      inFlight = true;
      try {
        const balance = await api.getWalletBalance(accountId);
                if (!cancelled)
                    setBalanceResult({ accountId, state: { status: 'ready', balance: balance ?? '0' } });
            }
            catch (e) {
                if (!cancelled)
                    setBalanceResult({ accountId, state: { status: 'error', message: sanitizeError(e) } });
            }
            finally {
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
    useEffect(() => {
        let cancelled = false, inFlight = false;
        const read = async () => { if (!nodeReady || inFlight)
            return; inFlight = true; try {
            const value = await api.getTransferPolicy();
            if (!cancelled)
                setPolicy(value);
        }
        catch (e) {
            if (!cancelled) {
                setPolicy({ executionFee: false, batchEnabled: false });
                setError(sanitizeError(e));
            }
        }
        finally {
            inFlight = false;
        } };
        void read();
        const timer = setInterval(() => void read(), 5000);
        return () => { cancelled = true; clearInterval(timer); };
    }, [nodeReady]);
    const handleTransferSubmit = useCallback(async (draft: TransferDraft) => {
        if (!activeWallet || preparingRef.current)
            return;
        const wallet = { ...activeWallet }, executionFee = policy.executionFee;
        preparingRef.current = true;
        setPreparing(true);
        setError(null);
        try {
            const prepared = await api.prepareTransfer(wallet.accountId, draft.mode, draft.transfers);
            if (quoteTransfers(prepared.transfers).fee.toString() !== prepared.feeFen)
                throw Error('手续费报价不一致');
            setSigningFlow({ mode: draft.mode, transfers: prepared.transfers, wallet, executionFee });
        }
        catch (e) {
            setError(sanitizeError(e));
        }
        finally {
            preparingRef.current = false;
            setPreparing(false);
        }
    }, [activeWallet, policy.executionFee]);
    const handleTransferSuccess = useCallback((result: TransferSubmitResult) => {
        if (!signingFlow)
            return;
        setPending(prev => [...prev, { hash: result.txHash, cursor: result.scanFrom, draft: { mode: signingFlow.mode, transfers: signingFlow.transfers } }]);
        setResetVersion(value => value + 1);
        if (signingFlow.mode !== 'transfer')
            setSigningFlow(null);
        setSuccessMsg(`转账已提交，哈希: ${result.txHash}`);
        if (successTimer.current)
            clearTimeout(successTimer.current);
    successTimer.current = setTimeout(() => setSuccessMsg(null), 5000);
    }, [signingFlow]);
    // 每次分页只读取最终块，读取失败保留游标和待确认状态，不重新付款。
    useEffect(() => {
        let cancelled = false, inFlight = false;
        const tick = async () => {
            if (inFlight || !nodeReady)
                return;
            inFlight = true;
            try {
                for (const tx of pending.filter(p => !p.result || p.result.state === 'pending')) {
                    try {
                        const result = await api.getTransferResult(tx.hash, tx.cursor);
                        if (!cancelled)
                            setPending(prev => prev.map(p => p.hash === tx.hash ? { ...p, cursor: result.cursor, result } : p));
                    }
                    catch (e) {
                        if (!cancelled)
                            setError(`结果待确认：${sanitizeError(e)}`);
                    }
                }
            }
            finally {
                inFlight = false;
            }
        };
        const timer = setInterval(() => void tick(), 3000);
        return () => { cancelled = true; clearInterval(timer); };
    }, [pending, nodeReady]);
    return (<div className="transaction-panel">
      <div className="transaction-panel-header">
        <div className="transaction-panel-heading">
          <h2>转账</h2>
          <MetricHelp title="链上转账说明" items={[
            '链上转账交易是指将交易发送到完全去中心化的公民区块链网络进行的转账交易。',
            '链上转账交易支持公民链和以太坊，可在收款地址栏输入公民链账户或以太坊账户。',
            '转账支持矿工热钱包或冷钱包，可在钱包管理中添加更多冷钱包，冷钱包使用扫码签名。',
            '签名交易完成后，等待权威节点完成最终性确认，最终性确认后，转账交易将永久固化。',
            '权威节点由1个国家储委会节点+43个省储委会节点组成，达30个节点验证即可形成最终性。',
          ]} />
        </div>
        <button className="wallet-manage-btn" onClick={() => setShowWalletModal(true)}>
          钱包管理
        </button>
      </div>

      {successMsg && <div className="transfer-success-msg">{successMsg}</div>}
      {pending.map(tx => <section className="transfer-result" key={tx.hash} aria-live="polite">
        <div>交易：{tx.hash}</div><div>{!tx.result || tx.result.state === 'pending' ? '等待最终确认' : tx.result.state === 'success' ? '全部成功' : tx.result.state === 'rolled_back' ? '整组回滚' : tx.result.state === 'partial' ? '部分成功' : '执行失败'}</div>
        {tx.result?.items?.map((item, index) => <div key={index}>第{index + 1}笔 {tx.draft.transfers[index]?.toAddress}：{item.state === 'success' ? '成功' : item.state === 'rolled_back' ? '整组回滚' : item.state === 'not_executed' ? '未执行' : '失败'}，手续费 {formatFen(item.feeFen)} 元</div>)}
        {tx.result?.feeFen != null && <div>实际手续费合计：{formatFen(tx.result.feeFen)} 元</div>}
        {tx.result?.error && <div className="error">{tx.result.error}</div>}
      </section>)}

      <TransferForm activeWallet={activeWallet} batchEnabled={policy.batchEnabled} nodeReady={nodeReady} balance={balanceState?.status === 'ready' ? balanceState.balance : null} balanceError={balanceState?.status === 'error' ? balanceState.message : null} onRetryBalance={() => setBalanceRetry((value) => value + 1)} onSubmit={handleTransferSubmit} disabled={signingFlow != null || preparing} submitError={error} resetVersion={resetVersion}/>

      <div className="transfer-principle-note">
        {/* 按指定断句固定分三行，保留原有居中样式。 */}
        <p>
          先有人类后有国家，是公民建立国家，国家是公民的国家，是公民治理国家，而不<br />
          是国家统治公民，公民没有爱国的义务。遵守公民宪法，加入公民链网络、使用<br />
          公民币，做自由公民。我们共同努力共建一个公开、公平、公正的公民社会。
        </p>
      </div>

      {signingFlow && (<TransferSigningFlow wallet={signingFlow.wallet} executionFee={signingFlow.executionFee} mode={signingFlow.mode} transfers={signingFlow.transfers} onClose={() => setSigningFlow(null)} onSuccess={handleTransferSuccess}/>)}

      {showWalletModal && (<WalletManagerModal wallets={wallets} activeAccountId={activeAccountId} onClose={() => setShowWalletModal(false)} onUpdate={handleWalletUpdate} onSelect={handleWalletSelect}/>)}
    </div>);
}
