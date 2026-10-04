// 私权机构本机构工作台。
//
// 本页面只展示当前登录机构自身信息与已授权模块，不提供注册局使用的省市筛选、
// 私权机构目录或其它机构详情入口。所有写权限仍由后端按准确机构 CID 独立校验。

import { Empty } from 'antd';
import type { AdminAuth } from '../auth/types';
import { OwnInstitutionAdminsView } from '../admins/RegistryAdminsView';
import { AccountManageSection } from '../accounts/AccountManageSection';
import { OwnInstitutionInfoPanel } from './judicial/JudicialDisplay';
import { PlatformPricePanel } from '../membership/PlatformPricePanel';
import { WorkspaceShell } from './WorkspaceShell';

export type PrivateInstitutionWorkspaceProps = {
  auth: AdminAuth;
};

function PrivateOperations({ auth }: PrivateInstitutionWorkspaceProps) {
  // 账户管理归本机构在册管理员;平台会员定价仅在后端开放该模块时叠加显示。
  const showPlatformPrice = auth.workspace?.workspace_modules.includes('platform_membership_price');
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <AccountManageSection auth={auth} />
      {showPlatformPrice ? <PlatformPricePanel auth={auth} /> : null}
    </div>
  );
}

function PrivateDisplay({ auth }: PrivateInstitutionWorkspaceProps) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <OwnInstitutionInfoPanel auth={auth} />
      {auth.capabilities?.canViewOwnAdmins ? (
        <OwnInstitutionAdminsView layout="cards" />
      ) : null}
    </div>
  );
}

function PrivateRecords() {
  return <Empty description="当前机构暂无操作记录" />;
}

export function PrivateInstitutionWorkspace({ auth }: PrivateInstitutionWorkspaceProps) {
  if (!auth.workspace || auth.workspace.workspace_kind !== 'private') return null;
  return (
    <WorkspaceShell
      workspace={auth.workspace}
      operations={<PrivateOperations auth={auth} />}
      display={<PrivateDisplay auth={auth} />}
      records={<PrivateRecords />}
    />
  );
}
