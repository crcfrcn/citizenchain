//! 既有业务入口向量，保持原权限条件及关闭状态。
use onchina::modules::capability::CapabilitySet;
use onchina::workspace::manifest::action;
use onchina::workspace::model::WorkspaceActionItem;
pub(super) fn operations(capabilities: CapabilitySet) -> Vec<WorkspaceActionItem> {
    vec![
        action(
            "register_citizen".to_string(),
            "公民登记",
            capabilities.can_view_citizens && capabilities.can_business_write,
        ),
        action(
            "register_institution".to_string(),
            "公权机构登记",
            capabilities.can_manage_institutions && capabilities.can_register_institutions,
        ),
        action(
            "register_private".to_string(),
            "私权机构登记",
            capabilities.can_view_private && capabilities.can_business_write,
        ),
        action(
            "register_education".to_string(),
            "教育机构登记",
            capabilities.can_view_education && capabilities.can_business_write,
        ),
        action(
            "manage_registry_admins".to_string(),
            "注册局管理员",
            capabilities.view_city_registry || capabilities.view_federal_registry,
        ),
    ]
}
