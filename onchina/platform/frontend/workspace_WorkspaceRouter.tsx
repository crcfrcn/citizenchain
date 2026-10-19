// 机构工作台路由。工作台类型由后端按准确机构 CID 下发，前端不得自行猜测权限。

import { Alert } from 'antd';
import type { AdminAuth } from './authentication_types';
import type { CapabilitySet } from './authentication_AuthContext';
import type { CidMetaResult } from './china_api';
import { GenericWorkspace } from './workspace_GenericWorkspace';
import { JudicialWorkspace } from '../../judicial/frontend/judicial_JudicialWorkspace';
import { PrivateInstitutionWorkspace } from '../../private/frontend/Workspace';
import { RegistryWorkspace } from '../../registry/frontend/Workspace';

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
