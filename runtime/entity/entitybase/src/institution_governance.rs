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
    InstitutionRoleStatus, RolePermissionOperation,
};

/// 普通机构政策扫描上限；任职总量另受实体的 MaxRoleEntries 限制。
pub const MAX_POLICY_ROLES: u32 = 64;
/// 单机构原生权限总量上限，覆盖原有单岗256项协议边界。
pub const MAX_POLICY_PERMISSIONS: u32 = 256;

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
/// 每位任职人员独立携带任期和来源，避免整体换届接口把存量成员错误改成同一任期。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub struct InstitutionAssignmentTarget {
    pub person_cid: Vec<u8>,
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
pub struct InstitutionRoleAssignmentChange {
    pub role_code: Vec<u8>,
    pub assignments: Vec<InstitutionAssignmentTarget>,
}

/// 动态岗位唯一允许的生命周期操作。
///
/// 创建时禁止提交岗位码；改名不能夹带权限或任期变化；删除后岗位码由
/// `UsedRoleCodes` 永久占用，不存在恢复或复用分支。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub enum InstitutionRoleMutation {
    /// 创建新动态岗位，并原子写入非空原生权限与初始任职。
    Create {
        role_name: Vec<u8>,
        term_required: bool,
        seats: u32,
        permissions: Vec<RolePermissionSpec>,
        assignments: Vec<InstitutionAssignmentTarget>,
    },
    /// 只修改现有动态岗位的公开名称。
    Rename {
        role_code: Vec<u8>,
        role_name: Vec<u8>,
    },
    /// 删除现有动态岗位、权限和当前任职；岗位码永久不释放。
    Delete { role_code: Vec<u8> },
    /// 调整普通动态岗位属性；任职变更必须在同一治理结果中满足目标属性。
    #[codec(index = 3)]
    Configure {
        role_code: Vec<u8>,
        term_required: bool,
        seats: u32,
        role_status: InstitutionRoleStatus,
    },
}

/// 普通动态岗位的完整目标原生权限，不能以空集合隐式删除链上岗位。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub struct RolePermissionChange {
    pub role_code: Vec<u8>,
    pub permissions: Vec<RolePermissionSpec>,
}

/// 机构自身治理的原生权限和共同阈值，不保存模块业务权限。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub struct InstitutionPolicyAction {
    pub permission_changes: Vec<RolePermissionChange>,
    pub threshold: Option<u32>,
}

/// 内部投票冻结的政策目标及原机构状态指纹；不建立新的政策存储。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub struct InstitutionPolicyProposal {
    pub institution_code: InstitutionCode,
    pub cid_number: Vec<u8>,
    pub action: InstitutionPolicyAction,
    pub expected_state_hash: [u8; 32],
}

/// 法定代表人公开信息目标变更。
///
/// 姓、名和人员 CID 一起设置或清空，并与法定代表人岗位任职一致。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub enum InstitutionLegalRepresentativeChange {
    /// 任命或更换法定代表人，完整公开人员字段必须同时写入。
    Set {
        family_name: Vec<u8>,
        given_name: Vec<u8>,
        person_cid: Vec<u8>,
    },
    /// 解除当前法定代表人，完整公开人员字段必须同时清空。
    Clear,
}

/// 业务模块交给 entity 的机构治理最终结果。
///
/// 一个结果可以同时调整多个岗位、多个岗位任职及法定代表人；entity 必须在同一
/// 存储事务内完成任职和法定代表人一致性校验及写入，任一步失败则全部回滚。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub struct InstitutionGovernanceResult {
    pub institution_code: InstitutionCode,
    /// 被治理机构的唯一身份 CID；不得使用主账户或任意账户代替。
    pub cid_number: Vec<u8>,
    /// 投票引擎实际分配的提案 ID；动态岗位码必须使用该值生成。
    pub proposal_id: u128,
    pub role_mutations: Vec<InstitutionRoleMutation>,
    pub assignment_changes: Vec<InstitutionRoleAssignmentChange>,
    pub legal_representative_change: Option<InstitutionLegalRepresentativeChange>,
    /// 指向产生本结果的登记、选举、投票或其他业务记录；不存在 `creator_account_id` 字段。
    pub result_source_ref: Vec<u8>,
}

/// 机构岗位、任职和法定代表人治理载荷。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub struct InstitutionGovernanceAction {
    pub role_mutations: Vec<InstitutionRoleMutation>,
    pub assignment_changes: Vec<InstitutionRoleAssignmentChange>,
    pub legal_representative_change: Option<InstitutionLegalRepresentativeChange>,
}

/// 注册局登记的岗位和任职变更。法定代表人任免走机构治理结果。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub struct InstitutionRoleRegistration {
    pub role_mutations: Vec<InstitutionRoleMutation>,
    pub assignment_changes: Vec<InstitutionRoleAssignmentChange>,
}

impl InstitutionRoleRegistration {
    /// 按编码长度和各类记录数计算登记工作量。
    pub fn registration_work(&self) -> (u32, u32, u32, u32) {
        let encoded_units =
            u32::try_from(self.encoded_size().saturating_add(255) / 256).unwrap_or(u32::MAX);
        let mut permissions = 0u32;
        let mut assignments = self.assignment_changes.iter().fold(0u32, |count, change| {
            count.saturating_add(change.assignments.len() as u32)
        });
        for mutation in &self.role_mutations {
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
            encoded_units,
            (self.role_mutations.len() as u32).saturating_add(self.assignment_changes.len() as u32),
            permissions,
            assignments,
        )
    }
}

/// 写入 ProposalData 的机构治理提案载荷。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Debug, TypeInfo, PartialEq, Eq)]
pub struct InstitutionGovernanceProposal {
    pub institution_code: InstitutionCode,
    pub cid_number: Vec<u8>,
    pub action: InstitutionGovernanceAction,
}

/// 已完成业务结果写入 entity 的唯一跨模块入口。
pub trait InstitutionGovernanceResultHandler {
    fn apply_institution_governance_result(
        result: InstitutionGovernanceResult,
    ) -> DispatchResult;
}

impl InstitutionGovernanceResultHandler for () {
    fn apply_institution_governance_result(
        _result: InstitutionGovernanceResult,
    ) -> DispatchResult {
        Err(sp_runtime::DispatchError::Other(
            "InstitutionGovernanceResultHandlerNotConfigured",
        ))
    }
}
