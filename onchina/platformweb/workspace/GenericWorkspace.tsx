// 通用机构工作台。未落专属 UI 的机构先使用三段式通用壳。

import { Empty } from 'antd';
import type { AdminAuth } from '../authentication/types';
import { OwnInstitutionAdminsView } from '../institutions/AdminsView';
import { AccountManageSection } from '../accounts/AccountManageSection';
import { moduleView } from './modules';
const LegislationView = (props: { auth: AdminAuth }) => { const View = moduleView<{ auth: AdminAuth }>('legislation', 'LegislationView'); return <View {...props} />; };
import { OwnInstitutionInfoPanel } from '../institutions/InfoPanel';
import { WorkspaceShell } from './WorkspaceShell';

export type GenericWorkspaceProps = {
  auth: AdminAuth;
};

function GenericOperations({ auth }: GenericWorkspaceProps) {
  // 账户管理归本机构在册管理员;立法与表决仅在后端开放能力位时叠加显示。
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <AccountManageSection auth={auth} />
      {auth.capabilities?.canViewLegislation ? <LegislationView auth={auth} /> : null}
    </div>
  );
}

function GenericDisplay({ auth }: GenericWorkspaceProps) {
  if (auth.capabilities?.canViewOwnAdmins) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
        <OwnInstitutionInfoPanel auth={auth} />
        <OwnInstitutionAdminsView layout="cards" />
      </div>
    );
  }
  return <Empty description="暂无可显示内容" />;
}

function GenericRecords() {
  return <Empty description="暂无记录" />;
}

export function GenericWorkspace({ auth }: GenericWorkspaceProps) {
  const workspace = auth.workspace;
  if (!workspace) return null;
  return (
    <WorkspaceShell
      workspace={workspace}
      operations={<GenericOperations auth={auth} />}
      display={<GenericDisplay auth={auth} />}
      records={<GenericRecords />}
    />
  );
}
