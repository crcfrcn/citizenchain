import { sanitizeError } from "../client/tauri";
import { homeNodeApi as api } from "./api";
import { ChainSection } from "./ChainSection";
import { IdentitySection } from "./IdentitySection";
import { IssuanceSection } from "./IssuanceSection";
import type { ChainStatus, NodeIdentity, NodeStatus, TotalIssuance, TotalStake, TotalFrozen } from "./types";
import { useCallback, useEffect, useRef, useState } from 'react';

const PARTIAL_REFRESH_ERROR_PREFIX = '部分数据刷新失败：';

const EMPTY_CHAIN: ChainStatus = {
  blockHeight: null, finalizedHeight: null, syncing: null,
  specVersion: null, transactionVersion: null, nodeVersion: '',
};
const EMPTY_IDENTITY: NodeIdentity = { peerId: null, role: null, genesisHash: null };
const EMPTY_ISSUANCE: TotalIssuance = { totalIssuance: null };
const EMPTY_STAKE: TotalStake = { totalStake: null };
const EMPTY_FROZEN: TotalFrozen = { totalFrozen: null };

// 仅在当前软件会话保留最后一次有效回执，切换 tab 不伪造节点停止。
let lastNodeStatus: NodeStatus | null = null;
// 页面重建及启停动作使旧请求失效，避免迟到回执覆盖当前状态。
let nodeStatusRevision = 0;

// 节点打开软件时自动启动；首页按钮仅提供手动停止和再次启动。
export function HomeNodeSection({ onNodeReadyChange }: { onNodeReadyChange: (ready: boolean) => void }) {
  const [status, setStatus] = useState<NodeStatus | null>(() => lastNodeStatus);
  const [chain, setChain] = useState<ChainStatus>(EMPTY_CHAIN);
  const [identity, setIdentity] = useState<NodeIdentity>(EMPTY_IDENTITY);
  const [minerAddress, setMinerAddress] = useState<string | null>(null);
  const [issuance, setIssuance] = useState<TotalIssuance>(EMPTY_ISSUANCE);
  const [stake, setStake] = useState<TotalStake>(EMPTY_STAKE);
  const [frozen, setFrozen] = useState<TotalFrozen>(EMPTY_FROZEN);
  const [error, setError] = useState<string | null>(null);
  const [nodeAction, setNodeAction] = useState<'starting' | 'stopping' | null>(null);
  const [pendingNodeAction, setPendingNodeAction] = useState<'start' | 'stop' | null>(null);
  const refreshInFlightRef = useRef(false);
  const mountedRef = useRef(false);
  const currentStatusRef = useRef(status);
  const statusRequestRef = useRef<Promise<NodeStatus | null> | null>(null);
  const nodeRunning = status?.running ?? false;
  const lifecycleBusy = nodeAction !== null
    || status?.state === 'starting'
    || status?.state === 'initializing'
    || status?.state === 'stopping'
    || status?.state === 'restarting';

  const nodeReady = nodeRunning && !lifecycleBusy;
  useEffect(() => {
    onNodeReadyChange(nodeReady);
  }, [nodeReady, onNodeReadyChange]);

  const clearHomeData = useCallback(() => {
    setChain(EMPTY_CHAIN);
    setIdentity(EMPTY_IDENTITY);
    setIssuance(EMPTY_ISSUANCE);
    setStake(EMPTY_STAKE);
    setFrozen(EMPTY_FROZEN);
  }, []);

  const applyNodeStatus = useCallback((nextStatus: NodeStatus) => {
    lastNodeStatus = nextStatus;
    currentStatusRef.current = nextStatus;
    setStatus(nextStatus);
    if (!nextStatus.running) clearHomeData();
  }, [clearHomeData]);

  const refreshNodeStatus = useCallback(() => {
    if (statusRequestRef.current) return statusRequestRef.current;
    const revision = nodeStatusRevision;
    const request = api.getNodeStatus().then((nextStatus) => {
      if (!mountedRef.current || revision !== nodeStatusRevision) return null;
      applyNodeStatus(nextStatus);
      return nextStatus;
    }).finally(() => {
      if (statusRequestRef.current === request) statusRequestRef.current = null;
    });
    statusRequestRef.current = request;
    return request;
  }, [applyNodeStatus]);

  const loadHome = useCallback(async (silent: boolean) => {
    const revision = nodeStatusRevision;
    const [s, c, i, t, k, f] = await Promise.allSettled([
      refreshNodeStatus(),
      api.getChainStatus(),
      api.getNodeIdentity(),
      api.getTotalIssuance(),
      api.getTotalStake(),
      api.getTotalFrozen(),
    ]);
    if (!mountedRef.current || revision !== nodeStatusRevision) return;
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

    // 节点回执已独立应用；其他数据不能把随后已停止的节点重新填成旧数据。
    applyResult(s, () => undefined);
    if (currentStatusRef.current && !currentStatusRef.current.running) {
      clearHomeData();
      if (s.status === 'rejected') {
        if (!silent) setError(`${PARTIAL_REFRESH_ERROR_PREFIX}${sanitizeError(s.reason)}`);
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
      return;
    }

    applyResult(c, setChain);
    applyResult(i, setIdentity);
    applyResult(t, setIssuance);
    applyResult(k, setStake);
    // 冻结总额必须完整取得；查询失败时清空旧金额，避免展示已经过期的合计。
    if (f.status === 'rejected') setFrozen(EMPTY_FROZEN);
    applyResult(f, setFrozen);

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
  }, [clearHomeData, refreshNodeStatus]);

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
    mountedRef.current = true;
    nodeStatusRevision += 1;
    statusRequestRef.current = null;
    // 独立发起，其他首页数据慢或尚未结束时也不阻塞真实状态。
    void refreshNodeStatus().catch(() => undefined);
    void runLoadHome(false).catch((e) => {
      if (mountedRef.current) setError(sanitizeError(e));
    });
    return () => { mountedRef.current = false; };
  }, [refreshNodeStatus, runLoadHome]);

  useEffect(() => {
    // 首次启动可能尚未生成地址，节点运行状态变化后重新读取本机账户。
    let cancelled = false;
    api.getLocalMinerSs58Address()
      .then((address) => {
        if (!cancelled) setMinerAddress(address);
      })
      .catch(() => {
        if (!cancelled) setMinerAddress(null);
      });
    return () => {
      cancelled = true;
    };
  }, [nodeRunning]);

  useEffect(() => {
    const timer = globalThis.setInterval(() => {
      void refreshNodeStatus().catch(() => undefined);
      if (refreshInFlightRef.current) {
        return;
      }
      void runLoadHome(true).catch(() => undefined);
    }, 3000);
    return () => globalThis.clearInterval(timer);
  }, [refreshNodeStatus, runLoadHome]);

  const requestNodeToggle = useCallback(() => {
    if (status === null || lifecycleBusy) return;
    setPendingNodeAction(status.running ? 'stop' : 'start');
  }, [lifecycleBusy, status]);

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
    let revision = ++nodeStatusRevision;
    statusRequestRef.current = null;
    try {
      const nextStatus = action === 'stop' ? await api.stopNode() : await api.startNode();
      if (!mountedRef.current || revision !== nodeStatusRevision) return;
      revision = ++nodeStatusRevision;
      statusRequestRef.current = null;
      applyNodeStatus(nextStatus);
      setNodeAction(null);
      if (nextStatus.running) await runLoadHome(false);
    } catch (e) {
      if (!mountedRef.current || revision !== nodeStatusRevision) return;
      revision = ++nodeStatusRevision;
      statusRequestRef.current = null;
      setError(sanitizeError(e));
      await refreshNodeStatus().catch(() => undefined);
      if (!mountedRef.current || revision !== nodeStatusRevision) return;
      setNodeAction(null);
      await runLoadHome(true).catch(() => undefined);
    } finally {
      if (mountedRef.current && revision === nodeStatusRevision) setNodeAction(null);
    }
  }, [applyNodeStatus, lifecycleBusy, pendingNodeAction, refreshNodeStatus, runLoadHome]);

  const nodeActionLabel = nodeAction === 'starting'
    ? '启动中...'
    : nodeAction === 'stopping'
      ? '关闭中...'
      : status?.state === 'starting' || status?.state === 'initializing' || status?.state === 'restarting'
        ? '启动中...'
        : status?.state === 'stopping'
          ? '关闭中...'
          : nodeRunning
            ? '关闭节点'
            : '启动节点';
  const statusLabel = nodeAction === 'stopping' || status?.state === 'stopping'
    ? '关闭中'
    : nodeAction === 'starting' || status?.state === 'initializing'
      || status?.state === 'starting' || status?.state === 'restarting'
      ? '启动中'
      : nodeRunning ? '运行中' : status === null ? null : '已停止';
  const confirmTitle = pendingNodeAction === 'stop' ? '确认关闭节点？' : '确认启动节点？';
  const confirmBody = pendingNodeAction === 'stop'
    ? '节点将停止运行，软件保持打开。'
    : status?.state === 'lock_held'
      ? '数据库锁可能仍被当前进程占用，若启动失败请完全退出软件后重新打开。'
      : '将启动本机节点。';
  const confirmButtonLabel = pendingNodeAction === 'stop' ? '确认关闭' : '确认启动';

  return (
    <>
      <div className="home-node-toolbar">
        <div className="status-line" style={{ visibility: statusLabel === null ? 'hidden' : undefined }}>
          <span className={`status-dot ${nodeRunning ? 'running' : 'stopped'}`} aria-hidden="true" />
          <span>状态：{statusLabel}</span>
        </div>
        <button
          type="button"
          className={`node-lifecycle-button ${nodeRunning ? 'stop' : 'start'}`}
          onClick={requestNodeToggle}
          disabled={status === null || lifecycleBusy}
          title={nodeActionLabel}
          aria-label={nodeActionLabel}
        />
      </div>
      <ChainSection chain={chain} nodeRunning={nodeRunning} />
      <IdentitySection identity={identity} minerAddress={minerAddress} />
      <IssuanceSection issuance={issuance} stake={stake} frozen={frozen} />
      {status?.lastError ? <pre className="error">{status.lastError}</pre> : null}
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
