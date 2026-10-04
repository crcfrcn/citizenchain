// 司法院操作页。当前只挂载已由后端能力位开放的司法类操作入口。

import { AuditOutlined, TeamOutlined } from '@ant-design/icons';
import { Button, Empty, Space } from 'antd';
import type { AdminAuth } from '../../auth/types';

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

