import { useCallback, useEffect, useRef, useState } from 'react';
import { sanitizeError } from '../tauri';
import { homeNodeApi as api } from './api';
import { ChainSection } from './components/ChainSection';
import { IdentitySection } from './components/IdentitySection';
import { IssuanceSection } from './components/IssuanceSection';
import type { ChainStatus, NodeIdentity, NodeStatus, TotalIssuance, TotalStake } from './types';

const PARTIAL_REFRESH_ERROR_PREFIX = '部分数据刷新失败：';

const EMPTY_CHAIN: ChainStatus = { blockHeight: null, finalizedHeight: null, syncing: null, specVersion: null, nodeVersion: '' };
const EMPTY_IDENTITY: NodeIdentity = { peerId: null, role: null, genesisHash: null };
const EMPTY_ISSUANCE: TotalIssuance = { totalIssuance: null };
const EMPTY_STAKE: TotalStake = { totalStake: null };

// 节点打开软件时自动启动；首页按钮仅提供手动停止和再次启动。
export function HomeNodeSection() {
  const [status, setStatus] = useState<NodeStatus>({ running: false, state: 'stopped', pid: null, lastError: null });
  const [chain, setChain] = useState<ChainStatus>(EMPTY_CHAIN);
  const [identity, setIdentity] = useState<NodeIdentity>(EMPTY_IDENTITY);
  const [issuance, setIssuance] = useState<TotalIssuance>(EMPTY_ISSUANCE);
  const [stake, setStake] = useState<TotalStake>(EMPTY_STAKE);
  const [error, setError] = useState<string | null>(null);
  const [nodeAction, setNodeAction] = useState<'starting' | 'stopping' | null>(null);
  const [pendingNodeAction, setPendingNodeAction] = useState<'start' | 'stop' | null>(null);
  const refreshInFlightRef = useRef(false);
  const lifecycleBusy = nodeAction !== null
    || status.state === 'starting'
    || status.state === 'initializing'
    || status.state === 'stopping'
    || status.state === 'restarting';

  const loadHome = useCallback(async (silent: boolean) => {
    const [s, c, i, t, k] = await Promise.allSettled([
      api.getNodeStatus(),
      api.getChainStatus(),
      api.getNodeIdentity(),
      api.getTotalIssuance(),
      api.getTotalStake(),
    ]);
    let successCount = 0;
    const failures: string[] = [];
    const applyResult = <T,>(
      result: PromiseSettledResult<T>,
      onFulfilled: (value: T) => void,
    ) => {
      if (result.status === 'fulfilled') {
        onFulfilled(result.value);
        successCount += 1;
        return;
      }
      failures.push(sanitizeError(result.reason));
    };

    applyResult(s, setStatus);
    if (s.status === 'fulfilled' && !s.value.running) {
      setChain(EMPTY_CHAIN);
      setIdentity(EMPTY_IDENTITY);
      setIssuance(EMPTY_ISSUANCE);
      setStake(EMPTY_STAKE);
      if (!silent) {
        setError(null);
        return;
      }
      setError((prev) => {
        if (!prev) return null;
        return prev.startsWith(PARTIAL_REFRESH_ERROR_PREFIX) ? null : prev;
      });
      return;
    }

    applyResult(c, setChain);
    applyResult(i, setIdentity);
    applyResult(t, setIssuance);
    applyResult(k, setStake);

    if (successCount === 0) {
      throw new Error(failures[0] ?? '首页数据加载失败');
    }
    if (failures.length > 0) {
      if (!silent) {
        setError(`${PARTIAL_REFRESH_ERROR_PREFIX}${failures[0]}`);
      }
      return;
    }

    if (!silent) {
      setError(null);
      return;
    }

    setError((prev) => {
      if (!prev) return null;
      return prev.startsWith(PARTIAL_REFRESH_ERROR_PREFIX) ? null : prev;
    });
  }, []);

  const resetHomeDataForStoppedNode = useCallback((nextStatus: NodeStatus) => {
    setStatus(nextStatus);
    setChain(EMPTY_CHAIN);
    setIdentity(EMPTY_IDENTITY);
    setIssuance(EMPTY_ISSUANCE);
    setStake(EMPTY_STAKE);
  }, []);

  const runLoadHome = useCallback(async (silent: boolean) => {
    if (refreshInFlightRef.current) return;
    refreshInFlightRef.current = true;
    try {
      await loadHome(silent);
    } finally {
      refreshInFlightRef.current = false;
    }
  }, [loadHome]);

  useEffect(() => {
    void runLoadHome(false).catch((e) => setError(sanitizeError(e)));
  }, [runLoadHome]);

  useEffect(() => {
    const timer = globalThis.setInterval(() => {
      if (refreshInFlightRef.current) {
        return;
      }
      void runLoadHome(true).catch(() => undefined);
    }, 3000);
    return () => globalThis.clearInterval(timer);
  }, [runLoadHome]);

  const requestNodeToggle = useCallback(() => {
    if (lifecycleBusy) return;
    setPendingNodeAction(status.running ? 'stop' : 'start');
  }, [lifecycleBusy, status.running]);

  const closeNodeConfirm = useCallback(() => {
    if (lifecycleBusy) return;
    setPendingNodeAction(null);
  }, [lifecycleBusy]);

  const handleConfirmedNodeAction = useCallback(async () => {
    if (!pendingNodeAction || lifecycleBusy) return;
    const action = pendingNodeAction;
    setPendingNodeAction(null);
    setNodeAction(action === 'stop' ? 'stopping' : 'starting');
    setError(null);
    try {
      const nextStatus = action === 'stop' ? await api.stopNode() : await api.startNode();
      if (!nextStatus.running) {
        resetHomeDataForStoppedNode(nextStatus);
        return;
      }
      setStatus(nextStatus);
      await runLoadHome(false);
    } catch (e) {
      setError(sanitizeError(e));
      await runLoadHome(true).catch(() => undefined);
    } finally {
      setNodeAction(null);
    }
  }, [lifecycleBusy, pendingNodeAction, resetHomeDataForStoppedNode, runLoadHome]);

  const nodeActionLabel = nodeAction === 'starting'
    ? '启动中...'
    : nodeAction === 'stopping'
      ? '关闭中...'
      : status.state === 'starting' || status.state === 'initializing' || status.state === 'restarting'
        ? '启动中...'
        : status.state === 'stopping'
          ? '关闭中...'
          : status.running
            ? '关闭'
            : '启动';
  const statusLabel = status.running
    ? '运行中'
    : status.state === 'initializing'
      ? '初始化中'
      : status.state === 'starting' || status.state === 'restarting'
        ? '启动中'
        : status.state === 'stopping'
          ? '关闭中'
          : status.state === 'lock_held'
            ? '数据库锁未释放'
            : status.state === 'failed'
              ? '启动失败'
              : status.state === 'exited'
                ? '异常退出'
                : '已停止';
  const confirmTitle = pendingNodeAction === 'stop' ? '确认关闭节点？' : '确认启动节点？';
  const confirmBody = pendingNodeAction === 'stop'
    ? '节点将停止运行，软件保持打开。'
    : status.state === 'lock_held'
      ? '数据库锁可能仍被当前进程占用，若启动失败请完全退出软件后重新打开。'
      : '将启动本机节点。';
  const confirmButtonLabel = pendingNodeAction === 'stop' ? '确认关闭' : '确认启动';

  return (
    <>
      <p className="status-line">
        <span className={`status-dot ${status.running ? 'running' : 'stopped'}`} />
        <span>状态: {statusLabel}</span>
        <button
          type="button"
          className={`node-lifecycle-button ${status.running ? 'stop' : 'start'}`}
          onClick={requestNodeToggle}
          disabled={lifecycleBusy}
          title={status.running ? '停止本机节点' : '启动本机节点'}
        >
          {nodeActionLabel}
        </button>
      </p>
      <ChainSection chain={chain} nodeRunning={status.running} />
      <IdentitySection identity={identity} />
      <IssuanceSection issuance={issuance} stake={stake} />
      {status.lastError ? <pre className="error">{status.lastError}</pre> : null}
      {error ? <pre className="error">{error}</pre> : null}
      {pendingNodeAction ? (
        <div className="node-lifecycle-confirm-mask" onClick={closeNodeConfirm}>
          <div
            className="node-lifecycle-confirm"
            role="dialog"
            aria-modal="true"
            aria-labelledby="node-lifecycle-confirm-title"
            onClick={(event) => event.stopPropagation()}
          >
            <h3 id="node-lifecycle-confirm-title">{confirmTitle}</h3>
            <p>{confirmBody}</p>
            <div className="node-lifecycle-confirm-actions">
              <button type="button" onClick={closeNodeConfirm} disabled={lifecycleBusy}>
                取消
              </button>
              <button type="button" onClick={handleConfirmedNodeAction} disabled={lifecycleBusy}>
                {confirmButtonLabel}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
}
