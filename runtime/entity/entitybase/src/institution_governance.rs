//! 定义机构治理结果、注册状态指纹和原子登记的共用协议。

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode};
use frame_support::dispatch::DispatchResult;
use frame_support::pallet_prelude::DecodeWithMemTracking;
use primitives::cid::code::InstitutionCode;
use scale_info::TypeInfo;
use sp_runtime::{
    traits::{BlakeTwo256, Hash},
    Debug,
};

use crate::{
    BusinessActionId, InstitutionAssignmentSource, InstitutionAssignmentStatus,
    RolePermissionOperation,
};
use adminbase::Admin;

/// Runtime与登记客户端共用的授权状态指纹；snapshot只含机构授权事实，余额不参与。
/// 各方必须使用相同字段顺序和有序岗位集合，不读取或创建任何storage。
pub fn registration_state_hash<Snapshot: Encode>(
    module_tag: &[u8],
    snapshot: &Snapshot,
) -> [u8; 32] {
    BlakeTwo256::hash_of(&(module_tag, snapshot)).into()
}

/// 来源绑定真实已签名登记内容和origin；Registry与治理提案引用分开解释。
pub fn registration_reference<Payload: Encode>(module_tag: &[u8], payload: &Payload) -> [u8; 32] {
    BlakeTwo256::hash_of(&(module_tag, InstitutionAssignmentSource::Registry, payload)).into()
}

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
    pub proposal_id: u128,
    pub role_mutations: Vec<InstitutionRoleMutation<AccountId>>,
    pub assignment_changes: Vec<InstitutionRoleAssignmentChange<AccountId>>,
    pub legal_representative_change: Option<InstitutionLegalRepresentativeChange<AccountId>>,
    /// 指向产生本结果的登记、选举、投票或其他业务记录；不存在 `creator_account_id` 字段。
    pub result_source_ref: Vec<u8>,
}

/// 机构管理员、岗位及任职的统一变更载荷。
///
/// `admins` 是机构可任职人员名册；岗位和任职是机构职务事实，岗位权限才是业务
/// 授权真源。call 8用于机构自身治理，call 9只接收合法登记子集；共享数据结构不代替
/// 各自入口的来源与权限校验。三者可以原子执行，但不能从另一方反向派生。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub enum InstitutionGovernanceAction<AccountId, AdminRecord = Admin<AccountId>> {
    /// 合法登记或机构内部投票通过后，完整替换机构 `admins` 真源。
    ReplaceAdmins { admins: Vec<AdminRecord> },
    /// 调整岗位定义、岗位任职和法定代表人，不改变机构 `admins`。
    MutateRolesAndAssignments {
        role_mutations: Vec<InstitutionRoleMutation<AccountId>>,
        assignment_changes: Vec<InstitutionRoleAssignmentChange<AccountId>>,
        legal_representative_change: Option<InstitutionLegalRepresentativeChange<AccountId>>,
    },
    /// 同一登记事务或治理提案内原子替换管理员并调整岗位/任职。
    ReplaceAdminsAndMutateRoles {
        admins: Vec<AdminRecord>,
        role_mutations: Vec<InstitutionRoleMutation<AccountId>>,
        assignment_changes: Vec<InstitutionRoleAssignmentChange<AccountId>>,
        legal_representative_change: Option<InstitutionLegalRepresentativeChange<AccountId>>,
    },
    /// 只轮换本机构既有管理员的签名钱包，不替换人员/任职；保持原变体0..2不变。
    RotateAdminWallet {
        change: adminbase::InstitutionAdminWalletChange<AccountId>,
        expires_at: u64,
        new_wallet_proof: adminbase::InstitutionWalletProof,
    },
}

impl<AccountId, AdminRecord> InstitutionGovernanceAction<AccountId, AdminRecord> {
    /// 登记只接收名册及显式岗位/任职变更，不夹带法定代表人任免。
    /// 首次岗位可随名册原子建立；成立后的权限修改仍由机构自身治理。
    pub fn into_registration_parts(
        self,
    ) -> Option<(
        Vec<AdminRecord>,
        Vec<InstitutionRoleMutation<AccountId>>,
        Vec<InstitutionRoleAssignmentChange<AccountId>>,
    )> {
        match self {
            Self::ReplaceAdmins { admins } => Some((admins, Vec::new(), Vec::new())),
            Self::ReplaceAdminsAndMutateRoles {
                admins,
                role_mutations,
                assignment_changes,
                legal_representative_change: None,
            } => Some((admins, role_mutations, assignment_changes)),
            _ => None,
        }
    }

    /// 只计数，不读取链状态；用于登记权重覆盖载荷中的岗位/权限/任职处理。
    pub fn registration_work(&self) -> (u32, u32, u32, u32)
    where
        Self: Encode,
    {
        // 按256字节计入额外编码工作，避免超长的无效输入只按一个岗位收费资源。
        let encoded_units =
            u32::try_from(self.encoded_size().saturating_add(255) / 256).unwrap_or(u32::MAX);
        let (admins, mutations, changes) = match self {
            Self::RotateAdminWallet { .. } => return (encoded_units, 0, 0, 0),
            Self::ReplaceAdmins { admins } => {
                return ((admins.len() as u32).max(encoded_units), 0, 0, 0)
            }
            Self::MutateRolesAndAssignments {
                role_mutations,
                assignment_changes,
                ..
            } => (0, role_mutations, assignment_changes),
            Self::ReplaceAdminsAndMutateRoles {
                admins,
                role_mutations,
                assignment_changes,
                ..
            } => (admins.len() as u32, role_mutations, assignment_changes),
        };
        let mut permissions = 0u32;
        let mut assignments = changes.iter().fold(0u32, |n, change| {
            n.saturating_add(change.assignments.len() as u32)
        });
        for mutation in mutations {
            if let InstitutionRoleMutation::Create {
                permissions: specs,
                assignments: targets,
                ..
            } = mutation
            {
                permissions = permissions.saturating_add(specs.len() as u32);
                assignments = assignments.saturating_add(targets.len() as u32);
            }
        }
        (
            admins.max(encoded_units),
            (mutations.len() as u32).saturating_add(changes.len() as u32),
            permissions,
            assignments,
        )
    }
}

impl<AccountId> InstitutionGovernanceAction<AccountId, Admin<AccountId>> {
    /// 在签名载荷和投票数据生成前补齐管理员默认姓名，保证提案与最终链上记录一致。
    pub fn normalize_admin_person_names(self) -> Self {
        match self {
            action @ Self::RotateAdminWallet { .. } => action,
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
