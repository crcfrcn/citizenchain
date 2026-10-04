// 通用机构工作台壳:只负责三段式导航和区块挂载,业务内容由各机构工作台注入。

import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { Segmented, Typography } from 'antd';
import type { InstitutionWorkspace, WorkspaceSectionKind } from './types';

export type WorkspaceShellProps = {
  workspace: InstitutionWorkspace;
  operations: ReactNode;
  display: ReactNode;
  records: ReactNode;
};

const sectionNodes: Record<WorkspaceSectionKind, keyof Pick<WorkspaceShellProps, 'operations' | 'display' | 'records'>> = {
  operations: 'operations',
  display: 'display',
  records: 'records',
};

export function WorkspaceShell({ workspace, operations, display, records }: WorkspaceShellProps) {
  const sections = useMemo(() => {
    return workspace.workspace_sections.length > 0
      ? workspace.workspace_sections
      : [
          { workspace_section: 'operations' as const, workspace_section_title: '操作', workspace_actions: [] },
          { workspace_section: 'display' as const, workspace_section_title: '显示', workspace_actions: [] },
          { workspace_section: 'records' as const, workspace_section_title: '记录', workspace_actions: [] },
        ];
  }, [workspace.workspace_sections]);
  const [activeSection, setActiveSection] = useState<WorkspaceSectionKind>(sections[0].workspace_section);

  useEffect(() => {
    setActiveSection(sections[0].workspace_section);
  }, [sections]);

  const bodyBySection = { operations, display, records };
  const activeNodeKey = sectionNodes[activeSection] ?? 'display';

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 16,
          flexWrap: 'wrap',
        }}
      >
        <Typography.Title level={3} style={{ margin: 0, color: '#ffffff' }}>
          {workspace.workspace_title}
        </Typography.Title>
        <Segmented
          value={activeSection}
          onChange={(value) => setActiveSection(value as WorkspaceSectionKind)}
          options={sections.map((section) => ({
            label: section.workspace_section_title,
            value: section.workspace_section,
          }))}
        />
      </div>
      <div>{bodyBySection[activeNodeKey]}</div>
    </div>
  );
}

