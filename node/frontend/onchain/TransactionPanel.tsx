// 组织钱包选择、余额及链上转账界面。
import { sanitizeError } from "../client/tauri";
import { transactionApi as api } from "./api";
import type { TransferDraft, Wallet, WalletStore, WalletBalanceState, TransferSubmitResult, TransactionCursor, TransactionRecord } from "./types";
import { WalletManagerModal } from "./WalletManagerModal";
import { TransferForm } from "./TransferForm";
import { TransferSigningFlow } from "./TransferSigningFlow";
import { accountIdToSs58 } from '../client/ss58';
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
  const [notice, setNotice] = useState<string | null>(null);
  const [resetVersion, setResetVersion] = useState(0);
    const [pending, setPending] = useState<{
        hash: string;
        cursor: number;
        draft: TransferDraft;
    }[]>([]);
  const [error, setError] = useState<string | null>(null);
  const noticeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const showNotice = useCallback((message:string) => {
    if(noticeTimer.current)clearTimeout(noticeTimer.current);
    setNotice(message);noticeTimer.current=setTimeout(()=>setNotice(null),3000);
  },[]);
  useEffect(()=>()=>{if(noticeTimer.current)clearTimeout(noticeTimer.current);},[]);
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
        if(signingFlow.mode==='transfer')showNotice('转账已提交');
    }, [signingFlow,showNotice]);
    // 每次分页只读取最终块，读取失败保留游标和待确认状态，不重新付款。
    useEffect(() => {
        if(!pending.length||!nodeReady)return;
        let cancelled = false, inFlight = false;
        const tick = async () => {
            if (inFlight || !nodeReady)
                return;
            inFlight = true;
            try {
                for (const tx of pending) {
                    try {
                        const result = await api.getTransferResult(tx.hash, tx.cursor);
                        if(cancelled)return;
                        if(result.state==='pending'){
                            setPending(previous=>previous.map(item=>item.hash===tx.hash?{...item,cursor:result.cursor}:item));
                        }else{
                            if(!result.items||result.items.length!==tx.draft.transfers.length||result.items.some(item=>!['success','failed','rolled_back','not_executed'].includes(item.state)))throw Error('结果子项不完整');
                            const success=result.items.filter(item=>item.state==='success').length;
                            showNotice(tx.draft.mode==='transfer'?(success===1?'转账成功':'转账失败'):`批量转账${result.items.length}笔，成功${success}笔，失败${result.items.length-success}笔。`);
                            setPending(previous=>previous.filter(item=>item.hash!==tx.hash));
                        }
                        setError(previous=>previous?.startsWith('结果待确认：')?null:previous);
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
    }, [pending, nodeReady,showNotice]);
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
        <div className="transaction-panel-actions">
          <button className="wallet-manage-btn" onClick={() => setShowWalletModal(true)}>钱包管理</button>
        </div>
      </div>

      {notice && <div className="transfer-notice" role="status">{notice}</div>}

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


// 独立tab仅在进入、刷新或加载更多时读取本机最终链，切走后丢弃迟到结果。
export function TransactionHistorySection(){
  const [wallets,setWallets]=useState<Wallet[]>([]);
  const [records,setRecords]=useState<TransactionRecord[]>([]);
  const [cursor,setCursor]=useState<TransactionCursor|null>(null);
  const [loading,setLoading]=useState(true),[error,setError]=useState<string|null>(null);
  const epoch=useRef(0),inFlight=useRef(false);
  const load=async(start:TransactionCursor|null,append:boolean)=>{
    if(inFlight.current)return;
    const id=++epoch.current;inFlight.current=true;setLoading(true);setError(null);
    if(!append){setRecords([]);setCursor(null);}
    try{
      if(!append){
        const store=await api.getWallets();
        if(id!==epoch.current)return;
        setWallets(store.wallets);
      }
      let position=start;
      while(true){
        const page=await api.getTransactionHistory(position);
        if(id!==epoch.current)return;
        if(page.cursor&&position&&(page.cursor.blockNumber>position.blockNumber||page.cursor.blockNumber===position.blockNumber&&(page.cursor.extrinsicIndex??Infinity)>=(position.extrinsicIndex??Infinity)))throw Error('交易记录游标未前进');
        if(page.records.length||page.cursor===null){setRecords(previous=>append?[...previous,...page.records]:page.records);setCursor(page.cursor);break;}
        position=page.cursor;
      }
    }catch(e){if(id===epoch.current)setError(sanitizeError(e));}
    finally{if(id===epoch.current){inFlight.current=false;setLoading(false);}}
  };
  useEffect(()=>{
    void load(null,false);
    return()=>{++epoch.current;inFlight.current=false;};
  },[]);
  const names=new Map(wallets.map(wallet=>[wallet.accountId,wallet.name]));
  const stateLabel:Record<string,string>={success:'成功',partial:'部分成功',failed:'失败',rolled_back:'已回滚'};
  return <section className="transaction-history-section" aria-labelledby="transaction-history-title">
      <header className="transaction-history-header"><h2 id="transaction-history-title">交易记录</h2><button className="wallet-manage-btn" disabled={loading} onClick={()=>void load(null,false)}>刷新</button></header>
      <div className="transaction-history-body">
        {error&&<div className="error" role="alert">{error}</div>}
        {records.length>0&&<table className="transaction-history-table"><thead><tr><th>时间</th><th>钱包</th><th>类型</th><th>金额</th><th>手续费</th><th>状态</th></tr></thead><tbody>{records.map(record=>{
          const type=record.direction==='received'?'转入':record.direction==='internal'?'内部转账':record.direction==='operation'?'链上交易':record.mode==='transfer'?'转出':record.mode==='other'?'转出':`批量转账（${record.transferCount}笔）`;
          const date=new Date(record.timestamp);
          return <tr key={`${record.blockNumber}:${record.txHash}`}><td>{Number.isNaN(date.getTime())?'—':date.toLocaleString()}</td><td>{record.accountIds.map(account=>names.get(account)??accountIdToSs58(account)).join('、')}</td><td>{type}</td><td>{record.direction==='received'?'+':record.direction==='sent'?'-':''}{formatFen(record.amountFen)} 元</td><td>{formatFen(record.feeFen)} 元</td><td>{stateLabel[record.state]??'未知'}</td></tr>;
        })}</tbody></table>}
        {loading&&<p role="status">读取中…</p>}
        {!loading&&!error&&records.length===0&&cursor===null&&<p className="no-data">暂无交易记录</p>}
      </div>
      {cursor!==null&&<button className="transaction-history-more" disabled={loading} onClick={()=>void load(cursor,true)}>加载更多</button>}
    </section>;
}
