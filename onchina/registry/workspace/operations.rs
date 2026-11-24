//! 按管理员权限生成注册局办理操作清单。

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
    ]
}
