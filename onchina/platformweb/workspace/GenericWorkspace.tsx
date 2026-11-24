// 按后端清单展示通用机构操作、资料和记录。

import { RolesView } from '../institutions/RolesView';
import { AssignmentsView } from '../institutions/AssignmentsView';
import { RepresentativeView } from '../institutions/RepresentativeView';
import { InstitutionDetailNavLayout } from './DetailLayout';
// 通用机构工作台。未落专属 UI 的机构先使用三段式通用壳。

import { Empty } from 'antd';
import type { AdminAuth } from '../authentication/types';
import { AccountManageSection } from '../accounts/AccountManageSection';
import { moduleView } from './modules';
const LegislationView = (props: { auth: AdminAuth }) => { const View = moduleView<{ auth: AdminAuth }>('legislation', 'LegislationView'); return <View {...props} />; };
import { OwnInstitutionInfoPanel } from '../institutions/InfoPanel';
import { WorkspaceShell } from './WorkspaceShell';

export type GenericWorkspaceProps = {
  auth: AdminAuth;
  focusLegislation?: boolean;
};

function GenericOperations({ auth, focusLegislation }: GenericWorkspaceProps) {
  // 账户管理归本机构在册管理员;立法与表决仅在后端开放能力位时叠加显示。
  const legislation = auth.capabilities?.canViewLegislation ? <LegislationView auth={auth} /> : null;
  return <InstitutionDetailNavLayout initialActiveKey={focusLegislation?'legislation':'accounts'} items={[
    {key:'accounts',label:'账户管理',content:<AccountManageSection auth={auth}/>},
    {key:'roles',label:'岗位管理',content:<RolesView auth={auth} cidNumber={auth.institutionCidNumber}/>},
    {key:'assignments',label:'任职管理',content:<AssignmentsView auth={auth} cidNumber={auth.institutionCidNumber}/>},
    {key:'representative',label:'法定代表人',content:<RepresentativeView auth={auth} cidNumber={auth.institutionCidNumber}/>},
    ...(legislation?[{key:'legislation',label:'立法业务',content:legislation}]:[]),
  ]}/>;
}

function GenericDisplay({ auth }: GenericWorkspaceProps) {
  return <OwnInstitutionInfoPanel auth={auth} />;
}

function GenericRecords() {
  return <Empty description="暂无记录" />;
}

export function GenericWorkspace({ auth, focusLegislation }: GenericWorkspaceProps) {
  const workspace = auth.workspace;
  if (!workspace) return null;
  return (
    <WorkspaceShell
      workspace={workspace}
      operations={<GenericOperations auth={auth} focusLegislation={focusLegislation} />}
      display={<GenericDisplay auth={auth} />}
      records={<GenericRecords />}
    />
  );
}
