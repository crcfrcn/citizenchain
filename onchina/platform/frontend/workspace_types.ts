// 机构工作台类型。字段名与后端 workspace DTO 保持 snake_case。

export type WorkspaceKind =
  | 'registry'
  | 'private'
  | 'judicial'
  | 'legislation'
  | 'public'
  | 'unincorporated';

/** 由后端按准确机构 CID 下发的实例级模块，前端不得按机构码自行推断。 */
export type WorkspaceModule = 'platform_membership_price';

export type WorkspaceSectionKind = 'operations' | 'display' | 'records';

export type WorkspaceAction =
  | 'register_citizen'
  | 'register_institution'
  | 'register_private'
  | 'register_education'
  | 'manage_registry_admins'
  | 'manage_own_admins'
  | 'view_own_admins'
  | 'view_legislation'
  | 'propose_legislation'
  | 'cast_representative_vote'
  | 'sign_legislation'
  | 'view_institution_profile'
  | 'view_operation_records';

export type WorkspaceActionItem = {
  action: WorkspaceAction;
  title: string;
  enabled: boolean;
};

export type WorkspaceSection = {
  workspace_section: WorkspaceSectionKind;
  workspace_section_title: string;
  workspace_actions: WorkspaceActionItem[];
};

export type InstitutionWorkspace = {
  workspace_kind: WorkspaceKind;
  workspace_title: string;
  workspace_sections: WorkspaceSection[];
  workspace_modules: WorkspaceModule[];
};
