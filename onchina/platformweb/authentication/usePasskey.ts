// 当前登录管理员的 passkey 注册状态 + 注册动作。
// 供管理员列表操作列「设置/更新 passkey 密钥」按钮(self-only)与红点使用。

import { useCallback, useState } from "react";
import { useAuth } from "./useAuth";
import { notice } from "../controls/notice";
import { registerPasskey } from "./passkeyClient";

interface UsePasskeyRegistration {
  /** 是否已注册 passkey;null=未知(加载中或出错)。 */
  registered: boolean | null;
  busy: boolean;
  register: () => Promise<void>;
  refresh: () => Promise<void>;
}

export function usePasskeyRegistration(): UsePasskeyRegistration {
  const { auth, passkeyState, refreshPasskey } = useAuth();
  const registered = passkeyState.status === 'ready' ? passkeyState.registered : null;
  const [busy, setBusy] = useState(false);

  const register = useCallback(async () => {
    if (!auth) return;
    setBusy(true);
    try {
      await registerPasskey(auth);
      // 工作台菜单和密钥按钮消费同一份状态，设置成功后同步刷新。
      await refreshPasskey();
      notice.success("passkey 密钥已设置");
    } catch (error) {
      notice.error(error, "passkey 设置失败");
    } finally {
      setBusy(false);
    }
  }, [auth, refreshPasskey]);

  return { registered, busy, register, refresh: refreshPasskey };
}
