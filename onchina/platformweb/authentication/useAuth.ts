// cid 前端登录态 hook。
// 步 0 起数据源统一到 <AuthProvider> / AuthContext,
// 本 hook 只暴露同一Context，包括登录态、能力与当前会话密钥状态。
// 业务子组件继续 import { useAuth } 即可。

import { useAuthContext, type AuthContextValue } from './Context';

export type UseAuthResult = AuthContextValue;

export function useAuth(): UseAuthResult {
  return useAuthContext();
}
