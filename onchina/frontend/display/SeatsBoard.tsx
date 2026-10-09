// 大屏席位板——本机构议员逐席投票色块(赞成绿/反对红/未投灰)+ 顶部图例计数。
// 只读呈现;数据来自后端逐席左连接投票映射(SeatView.vote:true/false/null)。

import React from 'react';
import type { SeatView } from './types';

interface Props {
  seats: SeatView[];
  approvedCount: number;
  rejectedCount: number;
  pendingCount: number;
}

/** 投票态 → 席位色板(边框 + 底色 + 文案)。语义色单源自此。 */
const VOTE_STYLE = {
  approved: { border: '#16a34a', bg: 'rgba(22,163,74,0.10)', dot: '#16a34a', label: '赞成' },
  rejected: { border: '#dc2626', bg: 'rgba(220,38,38,0.10)', dot: '#dc2626', label: '反对' },
  pending: { border: '#cbd5e1', bg: 'rgba(148,163,184,0.08)', dot: '#94a3b8', label: '未投' },
} as const;

type VoteKey = keyof typeof VOTE_STYLE;

function voteKey(vote: boolean | null): VoteKey {
  if (vote === true) return 'approved';
  if (vote === false) return 'rejected';
  return 'pending';
}

function LegendDot({ variant, count }: { variant: VoteKey; count: number }) {
  const s = VOTE_STYLE[variant];
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8, fontSize: 18 }}>
      <span
        style={{
          width: 14,
          height: 14,
          borderRadius: '50%',
          background: s.dot,
          display: 'inline-block',
        }}
      />
      <span style={{ color: '#334155' }}>
        {s.label} <strong style={{ color: '#0f172a' }}>{count}</strong>
      </span>
    </span>
  );
}

export function SeatsBoard({ seats, approvedCount, rejectedCount, pendingCount }: Props) {
  if (seats.length === 0) {
    return <div style={{ color: '#64748b', fontSize: 18 }}>本机构暂无在册议员。</div>;
  }
  return (
    <div>
      <div style={{ display: 'flex', gap: 28, marginBottom: 16 }}>
        <LegendDot variant="approved" count={approvedCount} />
        <LegendDot variant="rejected" count={rejectedCount} />
        <LegendDot variant="pending" count={pendingCount} />
      </div>
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))',
          gap: 14,
        }}
      >
        {seats.map((seat) => {
          const s = VOTE_STYLE[voteKey(seat.vote)];
          return (
            <div
              key={seat.account_id}
              style={{
                border: `1px solid ${s.border}`,
                borderLeft: `5px solid ${s.border}`,
                background: s.bg,
                borderRadius: 12,
                padding: '14px 16px',
                minHeight: 76,
              }}
            >
              <div style={{ fontSize: 22, fontWeight: 700, color: '#0f172a' }}>
                {seat.name || '未具名'}
              </div>
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                  marginTop: 6,
                }}
              >
                <span style={{ fontSize: 15, color: '#475569' }}>{seat.title || '委员'}</span>
                <span style={{ fontSize: 15, fontWeight: 600, color: s.dot }}>{s.label}</span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
