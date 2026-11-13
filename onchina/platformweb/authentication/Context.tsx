// cid 前端登录态 + 能力标志的全局 Context。
// 能力位由后端按已装载模块下发；所有模块复用本文件的同一Context实例。

import React, { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';
import type { AdminAuth } from './types';
import { setOnUnauthorized } from "../workspace/http";
import { clearStoredAuth, readStoredAuth, writeStoredAuth } from './storage';
import { notice } from '../controls/notice';
import { EMPTY_CAPABILITIES, type CapabilitySet } from '../workspace/modules';
import { getPasskeyStatus } from './passkeyClient';

// CapabilitySet 定义在 workspace/modules.ts;此处 re-export 保持既有引用路径。
export type { CapabilitySet };

export type PasskeyState =
  | { status: 'loading' }
  | { status: 'ready'; registered: boolean }
  | { status: 'error' };

/** 注册状态只属于当前准确会话；换令牌或机构时消费新登录快照，旧刷新不得覆盖。 */
function sameSession(left: AdminAuth | null, right: AdminAuth | null): boolean {
  return left !== null && right !== null
    && left.accessToken === right.accessToken
    && left.accountId === right.accountId
    && left.institutionCidNumber === right.institutionCidNumber
    && left.institutionCode === right.institutionCode;
}

/** 登录态 → 能力位:直接取后端会话下发的 capabilities(后端单源),未下发则空能力兜底。 */
export function resolveRoleCapabilities(auth: AdminAuth | null): CapabilitySet {
  return auth?.capabilities ?? EMPTY_CAPABILITIES;
}

export interface AuthContextValue {
  auth: AdminAuth | null;
  setAuth: (auth: AdminAuth | null) => void;
  logout: () => void;
  capabilities: CapabilitySet;
  passkeyState: PasskeyState;
  refreshPasskey: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export interface AuthProviderProps {
  children: React.ReactNode;
}

export const AuthProvider: React.FC<AuthProviderProps> = ({ children }) => {
  const [auth, setAuthState] = useState<AdminAuth | null>(() => readStoredAuth());
  const authRef = useRef(auth);
  authRef.current = auth;
  const passkeyRequest = useRef(0);
  const refreshPasskey = useCallback(async () => {
    const current = authRef.current;
    const requestId = ++passkeyRequest.current;
    if (!current) return;
    const registered = await getPasskeyStatus(current);
    if (requestId === passkeyRequest.current && sameSession(authRef.current, current)) {
      setAuthState(previous => sameSession(previous, current) && previous
        ? { ...previous, setupRequired: !registered } : previous);
    }
  }, []);

  // 登录/会话检查已返回初始化阶段；读取页面不再额外等待密钥查询。
  const passkeyState: PasskeyState = auth
    ? { status: 'ready', registered: !auth.setupRequired } : { status: 'loading' };

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
  }, [auth?.accessToken, auth?.accountId, auth?.institutionCidNumber, auth?.institutionCode]);

  // 换会话或卸载时丢弃尚未结束的状态刷新；不在切页时发起密钥查询。
  useEffect(() => {
    return () => { ++passkeyRequest.current; };
  }, [auth?.accessToken, auth?.accountId, auth?.institutionCidNumber, auth?.institutionCode, refreshPasskey]);

  const capabilities = useMemo(() => resolveRoleCapabilities(auth), [auth]);

  const value = useMemo<AuthContextValue>(
    () => ({ auth, setAuth, logout, capabilities, passkeyState, refreshPasskey }),
    [auth, setAuth, logout, capabilities, passkeyState, refreshPasskey],
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
