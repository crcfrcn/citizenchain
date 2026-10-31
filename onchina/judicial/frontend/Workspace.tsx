import type { AdminAuth } from "@gmb/onchina-platform/authentication/types";
import { AccountManageSection } from "@gmb/onchina-platform/accounts/AccountManageSection";
import { WorkspaceShell } from "@gmb/onchina-platform/workspace/WorkspaceShell";
import { OwnInstitutionAdminsView } from "@gmb/onchina-platform/institutions/AdminsView";
import { OwnInstitutionInfoPanel, type OwnInstitutionInfoProps } from "@gmb/onchina-platform/institutions/InfoPanel";
import { AuditOutlined, TeamOutlined } from "@ant-design/icons";
import { Button, Empty, Space } from "antd";

// 司法院工作台。非注册局机构不复用注册局 tab,统一按操作 / 显示 / 记录组织。








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

// 司法显示页组合原有公共面板，组件顺序、样式和行为保持不变。



export function JudicialDisplay({ auth }: OwnInstitutionInfoProps) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <OwnInstitutionInfoPanel auth={auth} />
      <OwnInstitutionAdminsView layout="cards" />
    </div>
  );
}

// 司法院操作页。当前只挂载已由后端能力位开放的司法类操作入口。





export type JudicialOperationsProps = {
  auth: AdminAuth;
};

export function JudicialOperations({ auth }: JudicialOperationsProps) {
  const actions = auth.workspace?.workspace_sections
    .find((section) => section.workspace_section === 'operations')
    ?.workspace_actions ?? [];

  if (actions.length === 0) {
    return <Empty description="暂无可执行操作" />;
  }

  return (
    <Space wrap>
      {actions.map((action) => {
        const icon = action.action === 'sign_legislation' ? <AuditOutlined /> : <TeamOutlined />;
        return (
          <Button
            key={action.action}
            icon={icon}
            disabled={!action.enabled}
            type={action.enabled ? 'primary' : 'default'}
          >
            {action.title}
          </Button>
        );
      })}
    </Space>
  );
}

// 司法院记录页。链上操作记录接入前只保留工作台分区位置。



export function JudicialRecords() {
  return <Empty description="暂无记录" />;
}
