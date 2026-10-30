//! 管理员要素、机构分类及固定人数策略。
use crate::*;

/// 分层强制单源真值表（= 运行期 Operation 的必填要素；account_id 恒必填不在表内）。
///
/// 门控在调用侧:仅 `ChainPhaseCheck::is_operation() == true` 时按本表强制;
/// Genesis（创世/开发期）一律放行=允许空。一次 runtime 升级把 Phase 翻 Operation
/// 即全域启用强制,无专属迁移。
///   - `PublicInstitution`            => 全 true（公权:所有管理员四要素完整）
///   - `PrivateInstitution` & LR 岗    => 全 true（私权:仅法定代表人岗四要素完整）
///   - `PrivateInstitution` & 非 LR 岗 => 全 false（仅 account_id）
///   - `PersonalMultisig`             => 全 false 【死规则,永不可翻真;个人多签禁强制姓/名/CID】
///
/// LR 岗判定单源:`role_code == primitives::institution_constraints::ROLE_CODE_LEGAL_REPRESENTATIVE`
/// (`b"LR"`),由持岗位任职上下文的 entity 层计算 `is_lr_role` 传入。
pub fn required_admin_elements(kind: AdminAccountKind, is_lr_role: bool) -> RequiredAdminElements {
    match kind {
        // 公权:所有管理员四要素完整。
        AdminAccountKind::PublicInstitution => RequiredAdminElements {
            cid: true,
            family: true,
            given: true,
        },
        // 私权:仅法定代表人(LR)岗四要素完整。
        AdminAccountKind::PrivateInstitution if is_lr_role => RequiredAdminElements {
            cid: true,
            family: true,
            given: true,
        },
        // 私权非 LR 岗:仅 account_id。
        AdminAccountKind::PrivateInstitution => RequiredAdminElements {
            cid: false,
            family: false,
            given: false,
        },
        // 个人多签:仅 account_id;cid 永 false【死规则,节点守卫锁死禁强制】。
        AdminAccountKind::PersonalMultisig => RequiredAdminElements {
            cid: false,
            family: false,
            given: false,
        },
    }
}

/// 判断机构码是否属于公权机构管理员模块。
pub fn is_public_admin_code(code: &InstitutionCode) -> bool {
    is_public_legal_code(code) || is_fixed_governance_code(code)
}

/// 判断机构码是否属于私权法人管理员模块。
pub fn is_private_admin_code(code: &InstitutionCode) -> bool {
    is_private_legal_code(code)
}

/// 判断机构码是否属于非法人机构管理员模块候选。
///
/// 非法人可隶属公法人或私法人;机构码本身不能决定管理员模块。
pub fn is_unincorporated_admin_code(code: &InstitutionCode) -> bool {
    is_unincorporated_code(code)
}

/// 判断机构码能否由公权管理员模块保存。
pub fn can_store_public_admin_code(code: &InstitutionCode) -> bool {
    is_public_admin_code(code) || is_unincorporated_admin_code(code)
}

/// 判断机构码能否由私权管理员模块保存。
pub fn can_store_private_admin_code(code: &InstitutionCode) -> bool {
    is_private_admin_code(code) || is_unincorporated_admin_code(code)
}

/// 判断机构码是否属于个人多签管理员模块。
pub fn is_personal_admin_code(code: &InstitutionCode) -> bool {
    *code == PMUL
}

/// 固定治理机构管理员人数；必须完整匹配机构码和 CID。
///
/// FRG 在 `admins` 中保存联邦注册局全部 215 名管理员；43 个省级 5 人岗位组
/// 由 `entity` 任职关系表达，不再维护第二套管理员分组 storage。
pub fn expected_fixed_governance_admins_len(
    code: InstitutionCode,
    cid_number: &[u8],
) -> Option<u32> {
    primitives::governance_skeleton::fixed_institution_by_identity(code, cid_number)
        .map(|institution| institution.expected_len)
}
