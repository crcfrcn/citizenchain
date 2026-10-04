import type React from 'react';

/** 业务卡片统一毛玻璃风格,放在独立模块避免业务视图反向 import App.tsx 形成循环依赖。 */
export const glassCardStyle: React.CSSProperties = {
  background: 'rgba(255,255,255,0.92)',
  backdropFilter: 'blur(16px)',
  borderRadius: 16,
  boxShadow: '0 4px 24px rgba(0,0,0,0.06)',
  border: '1px solid rgba(255,255,255,0.6)'
};

/** Card title 左侧 teal 竖条 + 加粗,供各业务一级目录复用。 */
export const glassCardHeadStyle: React.CSSProperties = {
  borderBottom: '1px solid #e5e7eb',
  borderLeft: '3px solid #0d9488',
  paddingLeft: 20,
  fontWeight: 600
};
