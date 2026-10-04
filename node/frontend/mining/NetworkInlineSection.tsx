// 挖矿页内嵌的网络统计面板：治理节点、在线节点两张并排卡片。
// 位置：由 MiningDashboardSection 注入在收益看板与出块记录之间。
// 设计决策：
//   - 治理节点：国家储委会 ｜ 省储委会 ｜ 省储行
//   - 在线节点：在线节点 ｜ 全节点 ｜ 轻节点
// 数据轮询：独立 5 秒间隔，与 MiningDashboardSection 的 10 秒收益轮询解耦，
// 因为网络拓扑变化频率高于收益刷新频率。
import { useCallback, useEffect, useRef, useState } from 'react';
import { sanitizeError } from '../tauri';
import { miningApi as api } from './api';
import type { NetworkOverview } from './types';

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

  // 两张卡片共用三列结构：每列一个数字和对应节点类型。
  const governanceCols: Array<{ name: string; value: number }> = [
    { name: '国家储委会', value: network.nrcNodes },
    { name: '省储委会', value: network.prcNodes },
    { name: '省储行', value: network.prbNodes },
  ];
  const onlineCols: Array<{ name: string; value: number }> = [
    { name: '在线节点', value: network.onlineNodes },
    { name: '全节点', value: network.fullNodes },
    { name: '轻节点', value: network.lightNodes },
  ];

  return (
    <section className="section network-inline-section">
      <h2>网络</h2>
      <div className="network-inline-grid">
        <div className="metric-card">
          <div className="metric-label">治理节点</div>
          <div className="network-node-grid">
            {governanceCols.map((col) => (
              <div key={col.name} className="network-node-col">
                <div className="network-node-value">
                  {loading ? '—' : col.value}
                </div>
                <div className="network-node-name">{col.name}</div>
              </div>
            ))}
          </div>
        </div>
        <div className="metric-card">
          <div className="metric-label">在线节点</div>
          <div className="network-node-grid">
            {onlineCols.map((col) => (
              <div key={col.name} className="network-node-col">
                <div className="network-node-value">
                  {loading ? '—' : col.value}
                </div>
                <div className="network-node-name">{col.name}</div>
              </div>
            ))}
          </div>
        </div>
      </div>
      {network.warning ? <pre className="warning">{network.warning}</pre> : null}
      {error ? <pre className="error">{error}</pre> : null}
    </section>
  );
}
