//! 按机构码选择工作台类型，供界面组织使用。

use super::model::WorkspaceKind;

/// 当前机构码对应的工作台类型。
pub(crate) fn workspace_kind_for(
    catalog: &crate::modules::ModuleCatalog,
    institution_code: &str,
) -> WorkspaceKind {
    if let Some(workspace) = catalog.workspace(institution_code) {
        return workspace.kind;
    }
    let Some(code) = primitives::cid::code::institution_code_from_str(institution_code) else {
        return WorkspaceKind::Public;
    };
    if primitives::cid::code::is_private_legal_code(&code) {
        return WorkspaceKind::Private;
    }
    if primitives::cid::code::is_unincorporated_code(&code) {
        return WorkspaceKind::Unincorporated;
    }
    WorkspaceKind::Public
}
