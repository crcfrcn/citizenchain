// 按后端清单展示通用机构操作、资料和记录。

import { RolesView } from '../institutions/RolesView';
import { AssignmentsView } from '../institutions/AssignmentsView';
import { RepresentativeView } from '../institutions/RepresentativeView';
import { InstitutionDetailNavLayout } from './DetailLayout';
// 通用机构工作台。未落专属 UI 的机构先使用三段式通用壳。

import { Empty } from 'antd';
import type { AdminAuth } from '../authentication/types';
import { AccountManageSection } from '../accounts/AccountManageSection';
import { moduleEntries, moduleView, resolveModuleLanding } from './modules';
import { OwnInstitutionInfoPanel } from '../institutions/InfoPanel';
import { WorkspaceShell } from './WorkspaceShell';

export type GenericWorkspaceProps = {
  auth: AdminAuth;
  requestedModule?: string | null;
};

function GenericOperations({ auth, requestedModule }: GenericWorkspaceProps) {
  // 通用账户及岗位入口保留；内嵌业务页由实际模块声明提供，不编入模块名单。
  const pages = moduleEntries(auth).filter(asset => !asset.workspace
    && resolveModuleLanding(auth, asset.id) === 'embedded').map(asset => {
    const View = moduleView<{ auth: AdminAuth }>(asset.id, asset.exports[0]);
    return { key: `module:${asset.id}`, label: asset.id, content: <View auth={auth} /> };
  });
  return <InstitutionDetailNavLayout initialActiveKey={requestedModule ? `module:${requestedModule}` : 'accounts'} items={[
    {key:'accounts',label:'账户管理',content:<AccountManageSection auth={auth}/>},
    {key:'roles',label:'岗位管理',content:<RolesView auth={auth} cidNumber={auth.institutionCidNumber}/>},
    {key:'assignments',label:'任职管理',content:<AssignmentsView auth={auth} cidNumber={auth.institutionCidNumber}/>},
    {key:'representative',label:'法定代表人',content:<RepresentativeView auth={auth} cidNumber={auth.institutionCidNumber}/>},
    ...pages,
  ]}/>;
}

function GenericDisplay({ auth }: GenericWorkspaceProps) {
  return <OwnInstitutionInfoPanel auth={auth} />;
}

function GenericRecords() {
  return <Empty description="暂无记录" />;
}

export function GenericWorkspace({ auth, requestedModule }: GenericWorkspaceProps) {
  const workspace = auth.workspace;
  if (!workspace) return null;
  return (
    <WorkspaceShell
      workspace={workspace}
      operations={<GenericOperations auth={auth} requestedModule={requestedModule} />}
      display={<GenericDisplay auth={auth} />}
      records={<GenericRecords />}
    />
  );
}
