// 向业务组件暴露同一认证Context中的登录态和能力。

import { useAuthContext, type AuthContextValue } from './Context';

export type UseAuthResult = AuthContextValue;

export function useAuth(): UseAuthResult {
  return useAuthContext();
}
