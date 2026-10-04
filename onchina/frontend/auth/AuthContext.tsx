// cid 前端登录态 + 能力标志的全局 Context。
// 能力位由机构码经 platform/capabilityMap 镜像后端权限派生(后端是唯一权限执行者)。

import React, { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';
import type { AdminAuth } from './types';
import { setOnUnauthorized } from '../utils/http';
import { clearStoredAuth, readStoredAuth, writeStoredAuth } from '../utils/storedAuth';
import { notice } from '../utils/notice';
import { EMPTY_CAPABILITIES, type CapabilitySet } from '../platform/capabilityMap';

// CapabilitySet 定义在 platform/capabilityMap;此处 re-export 保持既有引用路径。
export type { CapabilitySet };

/** 登录态 → 能力位:直接取后端会话下发的 capabilities(后端单源),未下发则空能力兜底。 */
export function resolveRoleCapabilities(auth: AdminAuth | null): CapabilitySet {
  return auth?.capabilities ?? EMPTY_CAPABILITIES;
}

export interface AuthContextValue {
  auth: AdminAuth | null;
  setAuth: (auth: AdminAuth | null) => void;
  logout: () => void;
  capabilities: CapabilitySet;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export interface AuthProviderProps {
  children: React.ReactNode;
}

export const AuthProvider: React.FC<AuthProviderProps> = ({ children }) => {
  const [auth, setAuthState] = useState<AdminAuth | null>(() => readStoredAuth());

  // auth 变化时同步 sessionStorage。null 时走 clearStoredAuth。
  useEffect(() => {
    if (auth) {
      writeStoredAuth(auth);
    } else {
      clearStoredAuth();
    }
  }, [auth]);

  const setAuth = useCallback((next: AdminAuth | null) => {
    setAuthState(next);
  }, []);

  const logout = useCallback(() => {
    setAuthState(null);
  }, []);

  // ── 401 拦截：token 失效时自动登出 + 提示 ──
  const logoutRef = useRef(logout);
  logoutRef.current = logout;
  useEffect(() => {
    setOnUnauthorized(() => {
      notice.warning('登录已过期，请重新登录');
      logoutRef.current();
    });
    return () => setOnUnauthorized(null);
  }, []);

  const capabilities = useMemo(() => resolveRoleCapabilities(auth), [auth]);

  const value = useMemo<AuthContextValue>(
    () => ({ auth, setAuth, logout, capabilities }),
    [auth, setAuth, logout, capabilities],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
};

export function useAuthContext(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error('useAuthContext 必须在 <AuthProvider> 内部使用');
  }
  return ctx;
}
