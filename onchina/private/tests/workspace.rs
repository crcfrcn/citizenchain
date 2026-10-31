//! 原私权工作台回归，参数与预期保持不变。
use onchina::workspace::build_institution_workspace;
use onchina::workspace::model::WorkspaceKind;

#[test]
fn private_legal_institution_gets_own_private_workspace() {
    let catalog = onchina::modules::test_support::fixture_catalog();
    let workspace = build_institution_workspace(
        &catalog,
        "SFGQ",
        Some("示例私权机构"),
        onchina::modules::capability::capabilities_for(&catalog, "SFGQ"),
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
    let catalog = onchina::modules::test_support::fixture_catalog();
    let registry = build_institution_workspace(
        &catalog,
        "FRG",
        None,
        onchina::modules::capability::capabilities_for(&catalog, "FRG"),
        Vec::new(),
    );
    let private = build_institution_workspace(
        &catalog,
        "SFGQ",
        None,
        onchina::modules::capability::capabilities_for(&catalog, "SFGQ"),
        Vec::new(),
    );
    assert_eq!(registry.workspace_kind, WorkspaceKind::Registry);
    assert_eq!(private.workspace_kind, WorkspaceKind::Private);
}
