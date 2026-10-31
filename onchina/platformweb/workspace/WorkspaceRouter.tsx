// 机构工作台路由。工作台类型由后端按准确机构 CID 下发，前端不得自行猜测权限。

import { Alert } from 'antd';
import type { AdminAuth } from '../authentication/types';
import type { CapabilitySet } from '../authentication/Context';
import type { CidMetaResult } from '../codes/api';
import { GenericWorkspace } from './GenericWorkspace';

import { workspaceView } from './modules';
export type WorkspaceRouterProps = {
  auth: AdminAuth;
  capabilities: CapabilitySet;
  passkeyRegistered: boolean | null;
  cidMeta: CidMetaResult | null;
  setCidMeta: (next: CidMetaResult | null) => void;
};

export function WorkspaceRouter({
  auth,
  capabilities,
  passkeyRegistered,
  cidMeta,
  setCidMeta,
}: WorkspaceRouterProps) {
  if (!auth.workspace) {
    return <Alert type="error" showIcon message="后端未返回机构工作台，已拒绝加载页面" />;
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
