// 43 省卡片网格 — 纯内容,标题栏由外层 Card 承载。

import React from 'react';
import type { CidProvinceItem } from '../china/api';

interface Props {
  provinces: CidProvinceItem[];
  onPick: (province_name: string) => void;
}

const CARD_STYLE: React.CSSProperties = {
  padding: 18,
  borderRadius: 12,
  border: '1px solid rgba(15,23,42,0.22)',
  background: 'rgba(226,232,240,0.55)',
  boxShadow: '0 2px 8px rgba(0,0,0,0.08)',
  cursor: 'pointer',
  transition: 'all 0.2s ease',
  textAlign: 'center' as const,
};

export const ProvinceGrid: React.FC<Props> = ({ provinces, onPick }) => {
  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(auto-fill, minmax(160px, 1fr))',
        gap: 12,
      }}
    >
      {provinces.map((p) => (
        <div
          key={p.province_code}
          onClick={() => onPick(p.province_name)}
          style={CARD_STYLE}
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLDivElement).style.background = 'rgba(13,148,136,0.22)';
            (e.currentTarget as HTMLDivElement).style.borderColor = 'rgba(13,148,136,0.55)';
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLDivElement).style.background = 'rgba(226,232,240,0.55)';
            (e.currentTarget as HTMLDivElement).style.borderColor = 'rgba(15,23,42,0.22)';
          }}
        >
          <div style={{ fontSize: 16, fontWeight: 600, color: '#0f172a' }}>{p.province_name}</div>
        </div>
      ))}
    </div>
  );
};
