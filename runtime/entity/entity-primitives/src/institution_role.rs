//! 机构岗位与管理员任职共用类型。
//!
//! 岗位属于机构实体，管理员账户集合属于 `admins` 模组；两者通过任职记录绑定。
//! 本文件只定义跨 pallet 共用的 SCALE 类型和只读查询接口，不持有任何 storage。

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::DecodeWithMemTracking;
use scale_info::TypeInfo;
use sp_runtime::{
    traits::{BlakeTwo256, Hash},
    Debug,
};

/// 岗位代码最大字节数。岗位代码是授权稳定键，不使用可变岗位名称进行鉴权。
pub const INSTITUTION_ROLE_CODE_MAX_BYTES: u32 = 64;

/// 任职来源引用最大字节数。
pub const ASSIGNMENT_SOURCE_REF_MAX_BYTES: u32 = 128;

/// 业务模块标签最大字节数，与 `VotePlan` 的跨端契约一致。
pub const BUSINESS_MODULE_TAG_MAX_BYTES: u32 = 32;

/// 单个岗位最多绑定的业务权限数量。
pub const MAX_ROLE_PERMISSIONS_PER_ROLE: u32 = 256;

/// 根据已通过提案生成动态岗位码。
///
/// 输出固定为 `R_` 加 32 位大写十六进制。nonce 由所属 entity pallet 单调递增，
/// 调用方没有任何提交或覆盖岗位码的入口。
///
/// `module_tag` 是所属 entity pallet 的 `MODULE_TAG`（`b"pub-mgmt"` / `b"pri-mgmt"`），
/// 承担哈希域分隔职责：全仓非签名哈希域一律用 `MODULE_TAG`，不另造版本化域常量。
pub fn generate_dynamic_role_code(
    module_tag: &[u8],
    cid_number: &[u8],
    nonce: u64,
    proposal_id: u64,
) -> Vec<u8> {
    let hash = BlakeTwo256::hash_of(&(module_tag, cid_number, nonce, proposal_id));
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut role_code = Vec::with_capacity(34);
    role_code.extend_from_slice(b"R_");
    for byte in &hash.as_ref()[..16] {
        role_code.push(HEX[(byte >> 4) as usize]);
        role_code.push(HEX[(byte & 0x0f) as usize]);
    }
    role_code
}

/// 机构岗位授权主体。
///
/// 授权必须同时包含机构 CID 与机构内岗位码；裸管理员账户、裸 CID 或裸岗位码都不能
/// 代替本类型。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct RoleSubject<CidNumber, RoleCode> {
    /// 机构唯一 CID。
    pub cid_number: CidNumber,
    /// 机构内唯一且永不复用的岗位码。
    pub role_code: RoleCode,
}

/// 业务动作稳定标识。
///
/// `module_tag` 绑定业务模块，`action_code` 绑定模块内具体动作；二者必须同时参与权限
/// 和投票计划校验。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct BusinessActionId<ModuleTag> {
    /// 业务模块现有 `MODULE_TAG`。
    pub module_tag: ModuleTag,
    /// 模块内稳定动作码，SCALE 固定按 `u32` 小端编码。
    pub action_code: u32,
}

/// 岗位对业务动作的权限操作。
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    TypeInfo,
    MaxEncodedLen,
    PartialEq,
    Eq,
)]
pub enum RolePermissionOperation {
    /// 发起该业务动作的提案。
    Propose,
    /// 参与该业务动作绑定的岗位投票。
    Vote,
}

/// 一条完整岗位业务权限。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct RoleBusinessPermission<CidNumber, RoleCode, ModuleTag> {
    /// 被授权的完整机构岗位主体。
    pub role_subject: RoleSubject<CidNumber, RoleCode>,
    /// 被授权的具体业务动作。
    pub business_action_id: BusinessActionId<ModuleTag>,
    /// 发起或投票权限。
    pub operation: RolePermissionOperation,
}

/// 投票与业务授权主体。
///
/// 机构岗位和个人多签使用不同枚举分支，禁止用个人多签账户伪装机构岗位主体。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub enum AuthorizationSubject<CidNumber, RoleCode, AccountId> {
    /// 机构 CID 与岗位码组成的岗位主体。
    Institution(RoleSubject<CidNumber, RoleCode>),
    /// 独立个人多签账户主体。
    PersonalMultisig(AccountId),
}

/// 机构岗位当前状态。
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    TypeInfo,
    MaxEncodedLen,
    PartialEq,
    Eq,
)]
pub enum InstitutionRoleStatus {
    /// 岗位可接受任职并参与业务授权。
    Active,
    /// 岗位已停用；历史任职仍由链上事件保留。
    Inactive,
}

/// 管理员任职来源。
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    TypeInfo,
    MaxEncodedLen,
    PartialEq,
    Eq,
)]
pub enum InstitutionAssignmentSource {
    /// 创世常量写入。
    Genesis,
    /// 注册局依法登记写入。
    Registry,
    /// 普选最终结果写入。
    PopularElection,
    /// 机构内部互选最终结果写入。
    MutualElection,
    /// 提名任免最终结果写入。
    NominationAppointment,
    /// 机构内部治理提案最终结果写入。
    InstitutionGovernance,
}

/// 管理员任职当前状态。
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    TypeInfo,
    MaxEncodedLen,
    PartialEq,
    Eq,
)]
pub enum InstitutionAssignmentStatus {
    /// 当前有效任职。
    Active,
    /// 任职已经结束。
    Ended,
}

/// 单个机构的岗位定义。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct InstitutionRole<CidNumber, RoleCode, RoleName> {
    /// 所属机构 CID。
    pub cid_number: CidNumber,
    /// 机构内唯一且创建后不可变的岗位代码。
    pub role_code: RoleCode,
    /// 公开岗位名称；只用于展示，不参与授权。
    pub role_name: RoleName,
    /// 是否强制要求非零且有效的开始、结束日期。
    pub term_required: bool,
    /// 岗位当前状态。
    pub role_status: InstitutionRoleStatus,
}

/// 管理员在某机构岗位上的任职事实。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, MaxEncodedLen, PartialEq, Eq,
)]
pub struct InstitutionAdminAssignment<CidNumber, AccountId, RoleCode, SourceRef> {
    /// 任职机构 CID。
    pub cid_number: CidNumber,
    /// 管理员唯一账户。
    pub account_id: AccountId,
    /// 所任岗位稳定代码。
    pub role_code: RoleCode,
    /// 任期开始日（自纪元起的天数）；无任期岗位固定为 0。
    pub term_start: u32,
    /// 任期结束日（自纪元起的天数）；无任期岗位固定为 0。
    pub term_end: u32,
    /// 任职来源。
    pub assignment_source: InstitutionAssignmentSource,
    /// 投票、选举、任命或注册结果的追溯引用；创世可为空。
    pub assignment_source_ref: SourceRef,
    /// 任职当前状态。
    pub assignment_status: InstitutionAssignmentStatus,
}

/// 机构岗位与任职只读接口。
///
/// 业务模块必须按“机构 CID + 稳定岗位代码 + 有效任职”鉴权，不能比较岗位名称。
pub trait InstitutionRoleQuery<AccountId> {
    /// 管理员是否在指定机构的指定岗位上有效任职。
    fn is_active_assignment(cid_number: &[u8], admin: &AccountId, role_code: &[u8]) -> bool;

    /// 读取指定机构、指定岗位的全部有效管理员账户。
    fn active_accounts_for_role(cid_number: &[u8], role_code: &[u8]) -> Vec<AccountId>;

    /// 读取某管理员在指定机构的全部有效岗位代码。
    fn active_role_codes(cid_number: &[u8], admin: &AccountId) -> Vec<Vec<u8>>;
}

/// CID 顶层业务能力策略。
///
/// entity 只调用本接口判断某个完整 CID 能否拥有目标权限；具体业务能力清单由 runtime
/// 组装和各业务模块登记，禁止下沉到投票引擎。
pub trait InstitutionCapabilityPolicy {
    fn allows(
        cid_number: &[u8],
        business_action_id: &BusinessActionId<Vec<u8>>,
        operation: RolePermissionOperation,
    ) -> bool;
}

impl InstitutionCapabilityPolicy for () {
    fn allows(
        _cid_number: &[u8],
        _business_action_id: &BusinessActionId<Vec<u8>>,
        _operation: RolePermissionOperation,
    ) -> bool {
        false
    }
}

/// 机构岗位业务授权统一查询入口。
pub trait InstitutionRoleAuthorizationQuery<AccountId> {
    /// 岗位主体是否持有目标业务权限，不判断具体账户任职。
    fn role_has_permission(
        role_subject: &RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &BusinessActionId<Vec<u8>>,
        operation: RolePermissionOperation,
    ) -> bool;

    /// 账户是否同时满足 admins、有效任职、CID 顶层能力和岗位权限。
    fn is_authorized(
        admin: &AccountId,
        role_subject: &RoleSubject<Vec<u8>, Vec<u8>>,
        business_action_id: &BusinessActionId<Vec<u8>>,
        operation: RolePermissionOperation,
    ) -> bool;

    /// 返回指定 CID 内持有目标业务权限的全部岗位主体。
    ///
    /// 实现必须按 `role_code` 确定性排序并去重；调用方仍需将结果限制在
    /// `VotePlan` 上限内。本查询只解析岗位权限，具体账户任职由投票引擎快照。
    fn role_subjects_with_permission(
        cid_number: &[u8],
        business_action_id: &BusinessActionId<Vec<u8>>,
        operation: RolePermissionOperation,
    ) -> Vec<RoleSubject<Vec<u8>, Vec<u8>>>;
}

impl<AccountId> InstitutionRoleAuthorizationQuery<AccountId> for () {
    fn role_has_permission(
        _role_subject: &RoleSubject<Vec<u8>, Vec<u8>>,
        _business_action_id: &BusinessActionId<Vec<u8>>,
        _operation: RolePermissionOperation,
    ) -> bool {
        false
    }

    fn is_authorized(
        _admin: &AccountId,
        _role_subject: &RoleSubject<Vec<u8>, Vec<u8>>,
        _business_action_id: &BusinessActionId<Vec<u8>>,
        _operation: RolePermissionOperation,
    ) -> bool {
        false
    }

    fn role_subjects_with_permission(
        _cid_number: &[u8],
        _business_action_id: &BusinessActionId<Vec<u8>>,
        _operation: RolePermissionOperation,
    ) -> Vec<RoleSubject<Vec<u8>, Vec<u8>>> {
        Vec::new()
    }
}

impl<AccountId> InstitutionRoleQuery<AccountId> for () {
    fn is_active_assignment(_cid_number: &[u8], _admin: &AccountId, _role_code: &[u8]) -> bool {
        false
    }

    fn active_accounts_for_role(_cid_number: &[u8], _role_code: &[u8]) -> Vec<AccountId> {
        Vec::new()
    }

    fn active_role_codes(_cid_number: &[u8], _admin: &AccountId) -> Vec<Vec<u8>> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Node Guard 按枚举声明序解码岗位和任职状态；任一重排都必须先修改节点协议。
    #[test]
    fn status_discriminants_match_governance_skeleton() {
        assert_eq!(
            InstitutionRoleStatus::Active as u8,
            primitives::governance_skeleton::ROLE_STATUS_ACTIVE
        );
        assert_eq!(
            InstitutionAssignmentStatus::Active as u8,
            primitives::governance_skeleton::ASSIGNMENT_STATUS_ACTIVE
        );
    }

    /// 权限操作和授权主体的枚举序号属于跨端 SCALE 契约。
    #[test]
    fn role_permission_discriminants_are_stable() {
        assert_eq!(RolePermissionOperation::Propose.encode(), vec![0]);
        assert_eq!(RolePermissionOperation::Vote.encode(), vec![1]);

        let institution =
            AuthorizationSubject::<Vec<u8>, Vec<u8>, [u8; 32]>::Institution(RoleSubject {
                cid_number: b"CID-1".to_vec(),
                role_code: b"ROLE-1".to_vec(),
            });
        let personal =
            AuthorizationSubject::<Vec<u8>, Vec<u8>, [u8; 32]>::PersonalMultisig([9u8; 32]);
        assert_eq!(institution.encode()[0], 0);
        assert_eq!(personal.encode()[0], 1);
    }

    #[test]
    fn dynamic_role_code_uses_exact_domain_and_never_accepts_lowercase() {
        let code = generate_dynamic_role_code(b"pub-mgmt", b"CID-1", 0, 42);
        assert_eq!(code.len(), 34);
        assert!(code.starts_with(b"R_"));
        assert!(code[2..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(byte)));
        assert_ne!(
            code,
            generate_dynamic_role_code(b"pub-mgmt", b"CID-1", 1, 42),
            "nonce 变化必须生成不同岗位码"
        );
        assert_ne!(
            code,
            generate_dynamic_role_code(b"pub-mgmt", b"CID-1", 0, 43),
            "proposal_id 变化必须生成不同岗位码"
        );
        assert_ne!(
            code,
            generate_dynamic_role_code(b"pri-mgmt", b"CID-1", 0, 42),
            "MODULE_TAG 变化必须生成不同岗位码(哈希域分隔)"
        );
    }

    /// 完整岗位业务权限的字段声明序就是五端共同使用的 SCALE 格式。
    #[test]
    fn role_business_permission_field_order_is_stable() {
        let permission = RoleBusinessPermission {
            role_subject: RoleSubject {
                cid_number: b"CID-1".to_vec(),
                role_code: b"ROLE-1".to_vec(),
            },
            business_action_id: BusinessActionId {
                module_tag: b"rt-upg".to_vec(),
                action_code: 7,
            },
            operation: RolePermissionOperation::Propose,
        };
        assert_eq!(
            permission.encode(),
            (
                (b"CID-1".to_vec(), b"ROLE-1".to_vec()),
                (b"rt-upg".to_vec(), 7u32),
                RolePermissionOperation::Propose,
            )
                .encode()
        );
    }

    /// 岗位字段声明序就是 `PublicManage::InstitutionRoles` 的链上 SCALE 值格式。
    #[test]
    fn institution_role_field_order_matches_node_guard() {
        let role = InstitutionRole {
            cid_number: b"CID-1".to_vec(),
            role_code: b"ROLE-1".to_vec(),
            role_name: "岗位一".as_bytes().to_vec(),
            term_required: true,
            role_status: InstitutionRoleStatus::Active,
        };
        assert_eq!(
            role.encode(),
            (
                b"CID-1".to_vec(),
                b"ROLE-1".to_vec(),
                "岗位一".as_bytes().to_vec(),
                true,
                InstitutionRoleStatus::Active,
            )
                .encode()
        );
    }

    /// 任职字段声明序就是 `PublicManage::InstitutionRoleAssignments` 元素的链上 SCALE 格式。
    #[test]
    fn institution_assignment_field_order_matches_node_guard() {
        let assignment = InstitutionAdminAssignment {
            cid_number: b"CID-1".to_vec(),
            account_id: [7u8; 32],
            role_code: b"ROLE-1".to_vec(),
            term_start: 10,
            term_end: 20,
            assignment_source: InstitutionAssignmentSource::PopularElection,
            assignment_source_ref: b"VOTE-1".to_vec(),
            assignment_status: InstitutionAssignmentStatus::Active,
        };
        assert_eq!(
            assignment.encode(),
            (
                b"CID-1".to_vec(),
                [7u8; 32],
                b"ROLE-1".to_vec(),
                10u32,
                20u32,
                InstitutionAssignmentSource::PopularElection,
                b"VOTE-1".to_vec(),
                InstitutionAssignmentStatus::Active,
            )
                .encode()
        );
    }
}
