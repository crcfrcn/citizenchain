// cid 前端登录态 hook。
// 步 0 起数据源统一到 <AuthProvider> / AuthContext,
// 本 hook 只是语义糖,返回 { auth, setAuth, logout } 以及 capabilities。
// 业务子组件继续 import { useAuth } 即可。

import type { AdminAuth } from '../auth/types';
import { useAuthContext, type CapabilitySet } from '../auth/AuthContext';

export interface UseAuthResult {
  auth: AdminAuth | null;
  setAuth: (auth: AdminAuth | null) => void;
  logout: () => void;
  capabilities: CapabilitySet;
}

export function useAuth(): UseAuthResult {
  const { auth, setAuth, logout, capabilities } = useAuthContext();
  return { auth, setAuth, logout, capabilities };
}
