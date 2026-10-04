//! 机构码到工作台类型的分类。
//!
//! 分类只决定前端壳子和页面组织方式;具体可执行能力仍以 `platform::capability` 下发的能力位为准。

use super::model::WorkspaceKind;

/// 当前机构码对应的工作台类型。
pub(crate) fn workspace_kind_for(institution_code: &str) -> WorkspaceKind {
    if crate::core::chain_runtime::is_tier1_registry(institution_code)
        || crate::core::chain_runtime::is_subordinate_registry(institution_code)
    {
        return WorkspaceKind::Registry;
    }
    if institution_code == "NJD" {
        return WorkspaceKind::Judicial;
    }
    if crate::domains::legislation::category::legislation_role(institution_code).is_some() {
        return WorkspaceKind::Legislation;
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
