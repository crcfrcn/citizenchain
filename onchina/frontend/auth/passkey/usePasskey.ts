// 当前登录管理员的 passkey 注册状态 + 注册动作。
// 供管理员列表操作列「设置/更新 passkey 密钥」按钮(self-only)与红点使用。

import { useCallback, useEffect, useState } from "react";
import { useAuth } from "../../hooks/useAuth";
import { notice } from "../../utils/notice";
import { getPasskeyStatus, registerPasskey } from "./passkeyClient";

interface UsePasskeyRegistration {
  /** 是否已注册 passkey;null=未知(加载中或出错)。 */
  registered: boolean | null;
  busy: boolean;
  register: () => Promise<void>;
  refresh: () => Promise<void>;
}

export function usePasskeyRegistration(): UsePasskeyRegistration {
  const { auth } = useAuth();
  const [registered, setRegistered] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    if (!auth) {
      setRegistered(null);
      return;
    }
    try {
      setRegistered(await getPasskeyStatus(auth));
    } catch {
      setRegistered(null);
    }
  }, [auth]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const register = useCallback(async () => {
    if (!auth) return;
    setBusy(true);
    try {
      await registerPasskey(auth);
      notice.success("passkey 密钥已设置");
      await refresh();
    } catch (error) {
      notice.error(error, "passkey 设置失败");
    } finally {
      setBusy(false);
    }
  }, [auth, refresh]);

  return { registered, busy, register, refresh };
}
