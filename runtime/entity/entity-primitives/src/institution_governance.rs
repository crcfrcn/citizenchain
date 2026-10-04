//! 机构治理结果共用协议。
//!
//! 具体业务只负责形成结果；`entity` 负责校验机构、岗位、任职、法定代表人和
//! `admins` 派生不变量。本协议不包含提名、选举、预算等业务规则，也不提供
//! 外部 extrinsic，因此新增或删除业务时不需要改变 entity storage。

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode};
use frame_support::dispatch::DispatchResult;
use frame_support::pallet_prelude::DecodeWithMemTracking;
use primitives::cid::code::InstitutionCode;
use scale_info::TypeInfo;
use sp_runtime::Debug;

use crate::{
    BusinessActionId, InstitutionAssignmentSource, InstitutionAssignmentStatus,
    RolePermissionOperation,
};
use admin_primitives::Admin;

/// 创建动态岗位时提交的权限规格。
///
/// 创建者不知道 runtime 即将生成的岗位码，因此这里只提交业务动作和操作；entity
/// 在执行通过提案时补齐完整 `RoleSubject`，形成 `RoleBusinessPermission`。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub struct RolePermissionSpec {
    pub business_action_id: BusinessActionId<Vec<u8>>,
    pub operation: RolePermissionOperation,
}

/// 单条目标任职。
///
/// 每个管理员独立携带任期和来源，避免整体换届接口把存量成员错误改成同一任期。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub struct InstitutionAssignmentTarget<AccountId> {
    pub account_id: AccountId,
    pub term_start: u32,
    pub term_end: u32,
    pub assignment_source: InstitutionAssignmentSource,
    pub assignment_source_ref: Vec<u8>,
    pub assignment_status: InstitutionAssignmentStatus,
}

/// 一个岗位的完整目标任职集合。
///
/// 未出现在结果中的岗位保持不变；出现在结果中的岗位按本集合整体替换。动态岗位
/// 可以提交空集合表示暂时空缺，固定创世岗位仍必须满足协议席位数。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub struct InstitutionRoleAssignmentChange<AccountId> {
    pub role_code: Vec<u8>,
    pub assignments: Vec<InstitutionAssignmentTarget<AccountId>>,
}

/// 动态岗位唯一允许的生命周期操作。
///
/// 创建时禁止提交岗位码；改名不能夹带权限或任期变化；删除后岗位码由
/// `UsedRoleCodes` 永久占用，不存在恢复或复用分支。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub enum InstitutionRoleMutation<AccountId> {
    /// 创建新动态岗位，并原子写入不可变权限与初始任职。
    Create {
        role_name: Vec<u8>,
        term_required: bool,
        permissions: Vec<RolePermissionSpec>,
        assignments: Vec<InstitutionAssignmentTarget<AccountId>>,
    },
    /// 只修改现有动态岗位的公开名称。
    Rename {
        role_code: Vec<u8>,
        role_name: Vec<u8>,
    },
    /// 删除现有动态岗位、权限和当前任职；岗位码永久不释放。
    Delete { role_code: Vec<u8> },
}

/// 法定代表人公开信息目标变更。
///
/// 三个字段只能整体设置或整体清空；没有“只改姓名/CID/账户”或使用管理员首位回退的路径。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub enum InstitutionLegalRepresentativeChange<AccountId> {
    /// 任命或更换法定代表人，完整公开人员字段必须同时写入。
    Set {
        family_name: Vec<u8>,
        given_name: Vec<u8>,
        cid_number: Vec<u8>,
        account_id: AccountId,
    },
    /// 解除当前法定代表人，完整公开人员字段必须同时清空。
    Clear,
}

/// 业务模块交给 entity 的机构治理最终结果。
///
/// 一个结果可以同时调整多个岗位、多个岗位任职及法定代表人；entity 必须在同一
/// storage transaction 内完成结果写入与 admins/任职一致性校验，任一步失败则全部回滚。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub struct InstitutionGovernanceResult<AccountId> {
    pub institution_code: InstitutionCode,
    /// 被治理机构的唯一身份 CID；不得使用主账户或任意账户代替。
    pub cid_number: Vec<u8>,
    /// 投票引擎实际分配的提案 ID；动态岗位码必须使用该值生成。
    pub proposal_id: u64,
    pub role_mutations: Vec<InstitutionRoleMutation<AccountId>>,
    pub assignment_changes: Vec<InstitutionRoleAssignmentChange<AccountId>>,
    pub legal_representative_change: Option<InstitutionLegalRepresentativeChange<AccountId>>,
    /// 指向产生本结果的登记、选举、投票或其他业务记录；不存在 `creator_account_id` 字段。
    pub result_source_ref: Vec<u8>,
}

/// 机构成立后的统一治理动作。
///
/// `admins` 是机构可任职人员名册；岗位和任职是机构职务事实，岗位权限才是业务
/// 授权真源。三者可以在同一 action 内原子执行，但任何一方都不能从另一方反向派生。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub enum InstitutionGovernanceAction<AccountId, AdminRecord = Admin<AccountId>> {
    /// 本机构内部投票通过后，完整替换机构 `admins` 真源。
    ReplaceAdmins { admins: Vec<AdminRecord> },
    /// 调整岗位定义、岗位任职和法定代表人，不改变机构 `admins`。
    MutateRolesAndAssignments {
        role_mutations: Vec<InstitutionRoleMutation<AccountId>>,
        assignment_changes: Vec<InstitutionRoleAssignmentChange<AccountId>>,
        legal_representative_change: Option<InstitutionLegalRepresentativeChange<AccountId>>,
    },
    /// 同一提案内原子替换管理员并调整岗位/任职。
    ReplaceAdminsAndMutateRoles {
        admins: Vec<AdminRecord>,
        role_mutations: Vec<InstitutionRoleMutation<AccountId>>,
        assignment_changes: Vec<InstitutionRoleAssignmentChange<AccountId>>,
        legal_representative_change: Option<InstitutionLegalRepresentativeChange<AccountId>>,
    },
}

impl<AccountId> InstitutionGovernanceAction<AccountId, Admin<AccountId>> {
    /// 在签名载荷和投票数据生成前补齐管理员默认姓名，保证提案与最终链上记录一致。
    pub fn normalize_admin_person_names(self) -> Self {
        match self {
            Self::ReplaceAdmins { admins } => Self::ReplaceAdmins {
                admins: admins.into_iter().map(Admin::normalize_names).collect(),
            },
            Self::MutateRolesAndAssignments {
                role_mutations,
                assignment_changes,
                legal_representative_change,
            } => Self::MutateRolesAndAssignments {
                role_mutations,
                assignment_changes,
                legal_representative_change,
            },
            Self::ReplaceAdminsAndMutateRoles {
                admins,
                role_mutations,
                assignment_changes,
                legal_representative_change,
            } => Self::ReplaceAdminsAndMutateRoles {
                admins: admins.into_iter().map(Admin::normalize_names).collect(),
                role_mutations,
                assignment_changes,
                legal_representative_change,
            },
        }
    }
}

/// 写入 ProposalData 的机构治理提案载荷。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub struct InstitutionGovernanceProposal<AccountId, AdminRecord = Admin<AccountId>> {
    pub institution_code: InstitutionCode,
    /// 被治理机构 CID。机构唯一主键只能是 CID，不能用机构账户替代。
    pub cid_number: Vec<u8>,
    pub action: InstitutionGovernanceAction<AccountId, AdminRecord>,
}

/// 已完成业务结果写入 entity 的唯一跨模块入口。
pub trait InstitutionGovernanceResultHandler<AccountId> {
    fn apply_institution_governance_result(
        result: InstitutionGovernanceResult<AccountId>,
    ) -> DispatchResult;
}

impl<AccountId> InstitutionGovernanceResultHandler<AccountId> for () {
    fn apply_institution_governance_result(
        _result: InstitutionGovernanceResult<AccountId>,
    ) -> DispatchResult {
        Err(sp_runtime::DispatchError::Other(
            "InstitutionGovernanceResultHandlerNotConfigured",
        ))
    }
}
