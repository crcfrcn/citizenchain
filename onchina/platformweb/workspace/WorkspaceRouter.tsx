// 机构工作台路由。工作台类型由后端按准确机构 CID 下发，前端不得自行猜测权限。

import { Alert, Button, Card } from 'antd';
import { useAuth } from '../authentication/useAuth';
import { usePasskeyRegistration } from '../authentication/usePasskey';
import { notice } from '../controls/notice';
import type { AdminAuth } from '../authentication/types';
import type { CapabilitySet } from '../authentication/Context';
import type { CidMetaResult } from '../codes/api';
import { GenericWorkspace } from './GenericWorkspace';

import { resolveModuleLanding, workspaceView } from './modules';
export type WorkspaceRouterProps = {
  auth: AdminAuth;
  requestedModule?: string | null;
  capabilities: CapabilitySet;
  passkeyRegistered: boolean | null;
  cidMeta: CidMetaResult | null;
  setCidMeta: (next: CidMetaResult | null) => void;
};

export function WorkspaceRouter({
  auth,
  requestedModule,
  capabilities,
  passkeyRegistered,
  cidMeta,
  setCidMeta,
}: WorkspaceRouterProps) {
  if (!auth.workspace) {
    return <Alert type="error" showIcon message="后端未返回机构工作台，已拒绝加载页面" />;
  }
  // 未完成首次设置只显示原密钥设置入口，不能先取名册或渲染业务页面。
  if (auth.setupRequired) return <PasskeySetup />;
  if (requestedModule != null) {
    switch (resolveModuleLanding(auth, requestedModule)) {
      case 'unavailable':
        return <Alert type="error" showIcon message="所选模块未加载或已不可用" />;
      case 'forbidden':
        return <Alert type="error" showIcon message="当前登录身份无权访问所选模块" />;
      case 'legislation':
        return <GenericWorkspace auth={auth} focusLegislation />;
      case 'workspace':
        break;
    }
  }

  const authWithWorkspace = auth;
  const RegisteredWorkspace = workspaceView<WorkspaceRouterProps>(authWithWorkspace);
  if (RegisteredWorkspace) {
    return (
      <RegisteredWorkspace
        auth={authWithWorkspace}
        capabilities={capabilities}
        passkeyRegistered={passkeyRegistered}
        cidMeta={cidMeta}
        setCidMeta={setCidMeta}
      />
    );
  }
  // 原专属工作台缺失时继续拒绝加载，不能静默改为别的机构页面。
  if (['registry', 'judicial', 'private'].includes(auth.workspace.workspace_kind)) {
    throw new Error('所需模块未装载');
  }
  return <GenericWorkspace auth={authWithWorkspace} />;
}

function PasskeySetup() {
  const { auth } = useAuth();
  const { busy, register, refresh } = usePasskeyRegistration();
  return <Card variant="borderless" title="请先设置 Passkey 密钥">
    <p>{auth?.cidShortName || '当前机构'} · {`${auth?.familyName ?? ''}${auth?.givenName ?? ''}` || '管理员'}</p>
    <p>首次登录必须完成密钥设置，设置成功后进入工作台。</p>
    <Button type="primary" loading={busy} onClick={() => void register()}>设置密钥</Button>
    <Button disabled={busy} onClick={() => void refresh().catch(error => notice.error(error, '密钥状态确认失败，请重试'))}>刷新状态</Button>
  </Card>;
}
