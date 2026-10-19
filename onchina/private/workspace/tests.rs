//! 原私权工作台回归，参数与预期保持不变。
use crate::workspace::build_institution_workspace;
use crate::workspace::model::{WorkspaceKind, WorkspaceModule};

#[test]
fn private_legal_institution_gets_own_private_workspace() {
    let workspace = build_institution_workspace(
        "SFGQ",
        Some("示例私权机构"),
        crate::modules::capability::capabilities_for("SFGQ"),
        vec!["platform_membership_price".to_string()],
    );
    assert_eq!(workspace.workspace_kind, WorkspaceKind::Private);
    assert_eq!(
        workspace.workspace_modules,
        vec!["platform_membership_price".to_string()]
    );
}

#[test]
fn registry_and_private_workspaces_never_share_kind() {
    let registry = build_institution_workspace(
        "FRG",
        None,
        crate::modules::capability::capabilities_for("FRG"),
        Vec::new(),
    );
    let private = build_institution_workspace(
        "SFGQ",
        None,
        crate::modules::capability::capabilities_for("SFGQ"),
        Vec::new(),
    );
    assert_eq!(registry.workspace_kind, WorkspaceKind::Registry);
    assert_eq!(private.workspace_kind, WorkspaceKind::Private);
}
