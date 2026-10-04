// 司法院工作台。非注册局机构不复用注册局 tab,统一按操作 / 显示 / 记录组织。

import type { AdminAuth } from '../../auth/types';
import { AccountManageSection } from '../../accounts/AccountManageSection';
import { WorkspaceShell } from '../WorkspaceShell';
import { JudicialDisplay } from './JudicialDisplay';
import { JudicialOperations } from './JudicialOperations';
import { JudicialRecords } from './JudicialRecords';

export type JudicialWorkspaceProps = {
  auth: AdminAuth;
};

export function JudicialWorkspace({ auth }: JudicialWorkspaceProps) {
  const workspace = auth.workspace;
  if (!workspace) return null;
  return (
    <WorkspaceShell
      workspace={workspace}
      operations={
        // 账户管理归本机构在册管理员;司法类操作入口叠加显示。
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
          <AccountManageSection auth={auth} />
          <JudicialOperations auth={auth} />
        </div>
      }
      display={<JudicialDisplay auth={auth} />}
      records={<JudicialRecords />}
    />
  );
}

