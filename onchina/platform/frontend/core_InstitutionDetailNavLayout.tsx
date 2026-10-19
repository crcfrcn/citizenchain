// 机构详情页共享左侧导航布局。这里只承载 BrixUI 风格 UI,不放业务 API。

import React, { useEffect, useMemo, useState } from 'react';
import {
  BankOutlined,
  FolderOpenOutlined,
  HistoryOutlined,
  HomeOutlined,
  TeamOutlined,
  WalletOutlined,
} from '@ant-design/icons';

export interface InstitutionDetailNavItem {
  key: string;
  label: string;
  content: React.ReactNode;
  badge?: React.ReactNode;
  icon?: React.ReactNode;
}

interface Props {
  items: InstitutionDetailNavItem[];
  backAction?: {
    label: string;
    onClick: () => void;
  };
  title?: React.ReactNode;
  subtitle?: React.ReactNode;
  status?: React.ReactNode;
  headerExtra?: React.ReactNode;
  /** 首屏默认选中的 section（须在 items 内,否则回退首项）。 */
  initialActiveKey?: string;
}

function defaultIcon(key: string) {
  switch (key) {
    case 'info':
      return <HomeOutlined />;
    case 'admins':
      return <TeamOutlined />;
    case 'accounts':
      return <WalletOutlined />;
    case 'documents':
      return <FolderOpenOutlined />;
    case 'operations':
      return <HistoryOutlined />;
    default:
      return <BankOutlined />;
  }
}

export const InstitutionDetailNavLayout: React.FC<Props> = ({
  items,
  backAction,
  title,
  subtitle,
  status,
  headerExtra,
  initialActiveKey,
}) => {
  const availableItems = useMemo(
    () => items.filter((item) => item.content !== null && item.content !== undefined),
    [items],
  );
  const [activeKey, setActiveKey] = useState(() => {
    if (initialActiveKey && availableItems.some((item) => item.key === initialActiveKey)) {
      return initialActiveKey;
    }
    return availableItems[0]?.key ?? '';
  });

  useEffect(() => {
    if (!availableItems.some((item) => item.key === activeKey)) {
      setActiveKey(availableItems[0]?.key ?? '');
    }
  }, [activeKey, availableItems]);

  const activeItem = availableItems.find((item) => item.key === activeKey) ?? availableItems[0];

  if (!activeItem) return null;

  const navItemBase: React.CSSProperties = {
    width: '100%',
    minHeight: 48,
    padding: '0 14px 0 12px',
    border: '1px solid transparent',
    background: 'transparent',
    color: '#475569',
    fontSize: 14,
    fontWeight: 500,
    textAlign: 'left',
    cursor: 'pointer',
    transition: 'background 0.16s ease, color 0.16s ease, box-shadow 0.16s ease',
    borderRadius: 8,
    display: 'flex',
    alignItems: 'center',
    gap: 10,
  };

  return (
    <div
      style={{
        width: '100%',
        padding: 14,
        borderRadius: 18,
        background: 'linear-gradient(180deg, rgba(240,253,250,0.96), rgba(248,250,252,0.96))',
        border: '1px solid rgba(204,251,241,0.8)',
        boxShadow: '0 16px 36px rgba(15,118,110,0.10)',
      }}
    >
      {(title || subtitle || status || headerExtra) && (
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: 16,
            padding: '8px 10px 18px',
          }}
        >
          <div style={{ minWidth: 0 }}>
            {title && (
              <TypographyTitle>
                {title}
              </TypographyTitle>
            )}
            {subtitle && (
              <div
                style={{
                  marginTop: 6,
                  color: '#64748b',
                  fontSize: 13,
                  wordBreak: 'break-all',
                }}
              >
                {subtitle}
              </div>
            )}
          </div>
          {(status || headerExtra) && (
            <div
              style={{
                flex: '0 0 auto',
                display: 'flex',
                alignItems: 'center',
                gap: 10,
              }}
            >
              {status}
              {headerExtra}
            </div>
          )}
        </div>
      )}

      <div
        style={{
          display: 'grid',
          gridTemplateColumns: '196px minmax(0, 1fr)',
          alignItems: 'stretch',
          width: '100%',
          overflowX: 'auto',
        }}
      >
        <aside
          style={{
            minWidth: 196,
            padding: '10px 0 10px 10px',
            background: 'rgba(226,232,240,0.42)',
            border: '1px solid #d7ede8',
            borderRight: 0,
            borderRadius: '14px 0 0 14px',
          }}
        >
          {backAction && (
            <button
              type="button"
              onClick={backAction.onClick}
              style={{
                ...navItemBase,
                marginBottom: 10,
                color: '#0f766e',
                background: 'rgba(13,148,136,0.10)',
              }}
            >
              <span style={{ width: 18, display: 'inline-flex', justifyContent: 'center' }}>
                ↩
              </span>
              <span>{backAction.label}</span>
            </button>
          )}
          {availableItems.map((item) => {
            const active = item.key === activeItem.key;
            return (
              <button
                key={item.key}
                type="button"
                onClick={() => setActiveKey(item.key)}
                style={{
                  ...navItemBase,
                  position: 'relative',
                  marginTop: 6,
                  marginRight: active ? -1 : 10,
                  background: active ? '#ffffff' : 'transparent',
                  borderColor: active ? '#d7ede8' : 'transparent',
                  borderRightColor: active ? '#ffffff' : 'transparent',
                  color: active ? '#0f766e' : '#475569',
                  borderRadius: active ? '10px 0 0 10px' : 10,
                  boxShadow: active ? '-3px 6px 18px rgba(15,118,110,0.10)' : 'none',
                }}
              >
                {active && (
                  <span
                    style={{
                      position: 'absolute',
                      left: 0,
                      top: 10,
                      bottom: 10,
                      width: 3,
                      borderRadius: 3,
                      background: '#0d9488',
                    }}
                  />
                )}
                <span
                  style={{
                    width: 22,
                    height: 22,
                    display: 'inline-flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    borderRadius: 6,
                    background: active ? 'rgba(13,148,136,0.12)' : 'rgba(100,116,139,0.10)',
                    color: active ? '#0f766e' : '#64748b',
                  }}
                >
                  {item.icon ?? defaultIcon(item.key)}
                </span>
                <span style={{ flex: '1 1 auto', minWidth: 0 }}>{item.label}</span>
                {item.badge && (
                  <span
                    style={{
                      flex: '0 0 auto',
                      minWidth: 24,
                      padding: '2px 7px',
                      borderRadius: 999,
                      background: active ? '#ccfbf1' : '#e2e8f0',
                      color: active ? '#0f766e' : '#64748b',
                      fontSize: 12,
                      textAlign: 'center',
                    }}
                  >
                    {item.badge}
                  </span>
                )}
              </button>
            );
          })}
        </aside>
        <section
          style={{
            minWidth: 0,
            padding: 18,
            border: '1px solid #d7ede8',
            background: '#ffffff',
            borderRadius: '0 14px 14px 0',
            boxShadow: '0 12px 28px rgba(15,23,42,0.06)',
          }}
        >
          {activeItem.content}
        </section>
      </div>
    </div>
  );
};

const TypographyTitle: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div
    style={{
      color: '#0f172a',
      fontSize: 22,
      fontWeight: 700,
      lineHeight: 1.25,
      wordBreak: 'break-word',
    }}
  >
    {children}
  </div>
);
