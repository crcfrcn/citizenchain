// 某省的 N 市卡片网格 — 共享组件。
// 按省读取确定性城市清单,优先命中本地缓存;点击某市回调外部。
// 过滤掉 city_code="000" 的"本省统一"占位。

import React, { useEffect, useState } from 'react';
import type { AdminAuth } from '../../platform/frontend/authentication_types';
import type { CidCityItem } from '../../platform/frontend/china_api';
import { loadCachedCidCities, readCachedCidCities } from '../../platform/frontend/china_metaCache';

interface Props {
  auth: AdminAuth;
  provinceName: string;
  onPick: (cityName: string) => void;
}

const CARD_STYLE: React.CSSProperties = {
  padding: 14,
  borderRadius: 10,
  border: '1px solid rgba(15,23,42,0.22)',
  background: 'rgba(226,232,240,0.55)',
  boxShadow: '0 2px 8px rgba(0,0,0,0.08)',
  cursor: 'pointer',
  transition: 'all 0.2s ease',
  textAlign: 'center' as const,
};

export const CityGrid: React.FC<Props> = ({ auth, provinceName, onPick }) => {
  const cachedCities = readCachedCidCities(provinceName);
  const [cities, setCities] = useState<CidCityItem[]>(
    () => cachedCities?.filter((c) => c.cityCode !== '000') ?? [],
  );
  const [loading, setLoading] = useState(!cachedCities);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const cachedRows = readCachedCidCities(provinceName);
    if (cachedRows) {
      setCities(cachedRows.filter((c) => c.cityCode !== '000'));
      setLoading(false);
    } else {
      setCities([]);
      setLoading(true);
    }
    setError(null);
    loadCachedCidCities(auth, provinceName)
      .then((rows) => {
        if (cancelled) return;
        setCities(rows.filter((c) => c.cityCode !== '000'));
      })
      .catch((err) => {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : '加载城市列表失败');
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [auth.accessToken, provinceName]);

  return (
    <div>
      {loading && <div style={{ color: 'rgba(15,23,42,0.6)' }}>加载中...</div>}
      {error && <div style={{ color: '#b00020' }}>{error}</div>}
      {!loading && !error && cities.length === 0 && (
        <div style={{ color: 'rgba(15,23,42,0.6)' }}>该省暂无城市数据</div>
      )}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(140px, 1fr))',
          gap: 10,
        }}
      >
        {cities.map((c) => (
          <div
            key={c.cityCode}
            onClick={() => onPick(c.cityName)}
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
            {/* 市名为空(字典/缓存未同步)时回退 city_code,绝不渲染空白细卡 */}
            <div style={{ fontSize: 15, fontWeight: 600, color: '#0f172a' }}>
              {c.cityName || c.cityCode}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};
