import { sanitizeError } from "../client/tauri";
import { miningApi as api } from "./api";
import type { NetworkOverview } from "./types";
// 挖矿页顶部的网络统计独立于收益数据，每 5 秒刷新。
import { useCallback, useEffect, useRef, useState } from 'react';

export function NetworkInlineSection() {
  const [network, setNetwork] = useState<NetworkOverview>({
    onlineNodes: 0,
    nrcNodes: 0,
    prcNodes: 0,
    prbNodes: 0,
    fullNodes: 0,
    lightNodes: 0,
    warning: null,
  });
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const mountedRef = useRef<boolean>(true);
  const requestIdRef = useRef<number>(0);

  const loadNetwork = useCallback(async () => {
    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;
    try {
      const data = await api.getNetworkOverview();
      if (!mountedRef.current || requestId !== requestIdRef.current) {
        return;
      }
      setNetwork(data);
      setError(null);
    } catch (e) {
      if (!mountedRef.current || requestId !== requestIdRef.current) {
        return;
      }
      setError(sanitizeError(e));
    } finally {
      if (mountedRef.current && requestId === requestIdRef.current) {
        setLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    void loadNetwork();
    const timer = globalThis.setInterval(() => {
      void loadNetwork();
    }, 5000);
    return () => {
      mountedRef.current = false;
      globalThis.clearInterval(timer);
    };
  }, [loadNetwork]);

  const nodes = [
    { name: '全部节点', value: network.onlineNodes },
    { name: '国家储委会', value: network.nrcNodes },
    { name: '省储委会', value: network.prcNodes },
    { name: '省储行', value: network.prbNodes },
    { name: '全节点', value: network.fullNodes },
    { name: '轻节点', value: network.lightNodes },
  ];

  return (
    <section aria-label="网络节点统计">
      <dl className="network-node-grid">
        {nodes.map(({ name, value }) => (
          <div key={name} className="network-node-item">
            <dt className="network-node-name">{name}</dt>
            <dd className="network-node-value">{loading ? '—' : value}</dd>
          </div>
        ))}
      </dl>
      {network.warning ? <pre className="warning">{network.warning}</pre> : null}
      {error ? <pre className="error">{error}</pre> : null}
    </section>
  );
}
