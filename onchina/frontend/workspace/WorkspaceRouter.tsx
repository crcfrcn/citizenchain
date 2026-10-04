// 机构工作台路由。工作台类型由后端按准确机构 CID 下发，前端不得自行猜测权限。

import { Alert } from 'antd';
import type { AdminAuth } from '../auth/types';
import type { CapabilitySet } from '../auth/AuthContext';
import type { CidMetaResult } from '../china/api';
import { GenericWorkspace } from './GenericWorkspace';
import { JudicialWorkspace } from './judicial/JudicialWorkspace';
import { PrivateInstitutionWorkspace } from './PrivateInstitutionWorkspace';
import { RegistryWorkspace } from './RegistryWorkspace';

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
  const workspaceKind = auth.workspace.workspace_kind;

  if (workspaceKind === 'registry') {
    return (
      <RegistryWorkspace
        auth={authWithWorkspace}
        capabilities={capabilities}
        passkeyRegistered={passkeyRegistered}
        cidMeta={cidMeta}
        setCidMeta={setCidMeta}
      />
    );
  }
  if (workspaceKind === 'judicial') {
    return <JudicialWorkspace auth={authWithWorkspace} />;
  }
  if (workspaceKind === 'private') {
    return <PrivateInstitutionWorkspace auth={authWithWorkspace} />;
  }
  return <GenericWorkspace auth={authWithWorkspace} />;
}
