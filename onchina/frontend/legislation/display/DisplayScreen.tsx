// 立法大屏只读入口(经 main.tsx `#/display` 分流,免登录、无 AuthProvider)。
// 轮询本节点绑定机构看板:院身份头 + 每个活跃立法提案(六阶段进度 + 逐席投票板)。
// 只读呈现;瞬时轮询失败保留上一份好数据 + 角标提示,不闪白。

import React, { useEffect, useRef, useState } from 'react';
import { Alert, Empty, Spin } from 'antd';
import { glassCardStyle, glassCardHeadStyle } from '../../core/cardStyles';
import { ProposalTallyPanel } from '../shared/ProposalTallyPanel';
import { getDisplayBoard } from './api';
import { SeatsBoard } from './SeatsBoard';
import type { DisplayBoard } from './types';

/** 大屏轮询间隔(毫秒)。链上表决为治理级低频事件,12s 足够实时且不压节点。 */
const REFRESH_MS = 12000;

const screenStyle: React.CSSProperties = {
  minHeight: '100vh',
  padding: '48px clamp(24px, 5vw, 96px)',
  background:
    'radial-gradient(1200px 600px at 20% -10%, rgba(13,148,136,0.14), transparent 60%),' +
    'radial-gradient(900px 500px at 100% 0%, rgba(37,99,235,0.10), transparent 55%),' +
    'linear-gradient(180deg, #f8fafc 0%, #eef2f7 100%)',
};

/** 顶栏统计块(名册规模 / 活跃提案数 / 刷新时间)。 */
function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <div style={{ textAlign: 'right' }}>
      <div style={{ fontSize: 15, color: '#64748b' }}>{label}</div>
      <div style={{ fontSize: 28, fontWeight: 700, color: '#0f172a' }}>{value}</div>
    </div>
  );
}

export function DisplayScreen() {
  const [board, setBoard] = useState<DisplayBoard | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [updatedAt, setUpdatedAt] = useState<string>('');
  const cancelled = useRef(false);

  useEffect(() => {
    cancelled.current = false;

    const load = async () => {
      try {
        const next = await getDisplayBoard();
        if (cancelled.current) return;
        setBoard(next);
        setError(null);
        setUpdatedAt(new Date().toLocaleTimeString('zh-CN', { hour12: false }));
      } catch (e: unknown) {
        if (cancelled.current) return;
        // 保留上一份好数据,仅角标提示瞬时失败。
        setError(e instanceof Error ? e.message : '看板刷新失败');
      }
    };

    void load();
    const timer = window.setInterval(() => void load(), REFRESH_MS);
    return () => {
      cancelled.current = true;
      window.clearInterval(timer);
    };
  }, []);

  if (!board && !error) {
    return (
      <div style={{ ...screenStyle, display: 'grid', placeItems: 'center' }}>
        <div style={{ textAlign: 'center' }}>
          <Spin size="large" />
          <div style={{ marginTop: 16, color: '#475569', fontSize: 18 }}>接入链上看板…</div>
        </div>
      </div>
    );
  }

  if (!board && error) {
    return (
      <div style={{ ...screenStyle, display: 'grid', placeItems: 'center' }}>
        <Alert type="error" showIcon message="无法加载大屏看板" description={error} />
      </div>
    );
  }

  const view = board as DisplayBoard;

  return (
    <div style={screenStyle}>
      <header
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'flex-end',
          gap: 24,
          flexWrap: 'wrap',
          marginBottom: 32,
        }}
      >
        <div>
          <h1
            style={{
              fontSize: 'clamp(32px, 4vw, 56px)',
              fontWeight: 800,
              color: '#0f172a',
              margin: 0,
            }}
          >
            {view.cidShortName || view.institutionCode}
          </h1>
          <div style={{ fontSize: 20, color: '#475569', marginTop: 6 }}>
            立法与表决 · {view.scopeLabel} · 机构码 {view.institutionCode}
          </div>
        </div>
        <div style={{ display: 'flex', gap: 40, alignItems: 'flex-end' }}>
          <Stat label="在册议员" value={view.rosterTotal} />
          <Stat label="活跃提案" value={view.activeProposals.length} />
          <Stat label="刷新于" value={updatedAt || '—'} />
        </div>
      </header>

      {error && (
        <Alert
          style={{ marginBottom: 24 }}
          type="warning"
          showIcon
          message={`看板刷新暂时失败,显示上次数据(${error})`}
        />
      )}

      {view.activeProposals.length === 0 ? (
        <div style={{ ...glassCardStyle, padding: 64 }}>
          <Empty description={<span style={{ fontSize: 20 }}>当前无活跃立法提案</span>} />
        </div>
      ) : (
        <main style={{ display: 'flex', flexDirection: 'column', gap: 28 }}>
          {view.activeProposals.map((item) => (
            <section
              key={item.state.proposalId}
              aria-label={`提案 #${item.state.proposalId}`}
              style={{ ...glassCardStyle, padding: 0 }}
            >
              <h2 style={{ ...glassCardHeadStyle, padding: '18px 24px', fontSize: 24, margin: 0 }}>
                提案 #{item.state.proposalId}
              </h2>
              <div
                style={{
                  padding: 24,
                  display: 'grid',
                  gridTemplateColumns: 'minmax(320px, 1fr) 2fr',
                  gap: 32,
                }}
              >
                <ProposalTallyPanel state={item.state} />
                <SeatsBoard
                  seats={item.seats}
                  approvedCount={item.approvedCount}
                  rejectedCount={item.rejectedCount}
                  pendingCount={item.pendingCount}
                />
              </div>
            </section>
          ))}
        </main>
      )}
    </div>
  );
}
