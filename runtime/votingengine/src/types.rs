//! 投票引擎核心数据结构与常量定义。
//!
//! 包括:提案 ID 别名、提案/投票各类计数 struct、提案状态阶段常量、
//! 内部提案互斥锁结构、执行重试状态、清理阶段枚举以及业务模块绑定的投票计划等。

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
pub use entity_primitives::{AuthorizationSubject, BusinessActionId, RoleSubject};
use frame_support::{
    pallet_prelude::{BoundedVec, ConstU32, DecodeWithMemTracking},
    traits::Get,
};
use scale_info::TypeInfo;

pub const PROPOSAL_KIND_INTERNAL: u8 = 0;
pub const PROPOSAL_KIND_JOINT: u8 = 1;
/// 立法投票 pallet 的提案类型(ADR-027)。立法机构专属:单院/两院内部表决 + 特别案强制公投。
pub const PROPOSAL_KIND_LEGISLATION: u8 = 2;
/// 选举投票 pallet 的提案类型。只承载选举流程,职位规则由业务模块/选举法解释后传入快照。
pub const PROPOSAL_KIND_ELECTION: u8 = 3;
// 机构码治理分类(全链唯一真源 = primitives::cid::code,见铁律)
// 治理统一用 CID 机构码 [u8; 4]。
// 这里整体 re-export,使 internal-vote / joint-vote / 业务 pallet 仍可写
// `votingengine::types::{InstitutionCode, NRC, fixed_governance_pass_threshold, ...}`。
pub use primitives::cid::code::{
    code_bytes, fixed_governance_pass_threshold, institution_code_from_cid_number,
    is_fixed_governance_code, is_institution_code, is_personal_code, is_public_legal_code,
    is_registered_multisig_code, is_valid_governance_code, InstitutionCode, FRG, NJD, NRC, PMUL,
    PRB, PRC,
};

/// 机构 CID 号链上固定上限。所有机构类主体以 CID 作为唯一身份真源。
pub type CidNumber = BoundedVec<u8, ConstU32<{ primitives::core_const::CID_NUMBER_MAX_BYTES }>>;

/// 机构岗位码。上限与 entity 岗位 storage 契约一致。
pub type RoleCode =
    BoundedVec<u8, ConstU32<{ entity_primitives::INSTITUTION_ROLE_CODE_MAX_BYTES }>>;

/// 机构岗位的一张投票票据。
///
/// 票权由机构 CID、岗位码和实际签名账户共同确定；同一账户担任多个岗位时，
/// 每个岗位各形成一张独立票据，同一岗位与同一账户组合只能使用一次。
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct InstitutionVoteTicket<AccountId> {
    pub role_subject: RoleSubject<CidNumber, RoleCode>,
    pub voter_account_id: AccountId,
}

/// 公民公投的一张完整票据。
///
/// Storage 必须按 `(proposal_id, cid_number)` 去重；票据值保存投票时
/// 的完整 CID + 签名账户主体，防止同一永久 CID 更换绑定账户后重复投票。
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct CitizenReferendumTicket<AccountId> {
    pub voter_subject: citizen_identity::CitizenSubject<AccountId>,
    pub approve: bool,
}

/// 投票引擎按提案保存的人口快照。
///
/// 人口数据全部来自 citizen-identity；投票引擎只增加创建区块并冻结为提案历史，
/// 不自行统计或解释人口。
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct ProposalPopulationSnapshot<BlockNumber> {
    pub population_data: citizen_identity::PopulationData,
    pub created_at: BlockNumber,
}

/// 全链投票计划使用的固定模块标签上限。
///
/// `JointVoteEngine` 是跨 pallet trait，不能携带 runtime 关联类型；因此接口使用与
/// `entity-primitives` 协议常量完全一致的固定上限。runtime 的 `MaxModuleTagLen` 必须
/// 保持同值，业务 owner 索引仍使用该关联类型。
pub type VotePlanOf<AccountId> =
    VotePlan<AccountId, ConstU32<{ entity_primitives::BUSINESS_MODULE_TAG_MAX_BYTES }>>;

/// 单个投票计划最多绑定的授权主体数量。
pub const MAX_VOTE_PLAN_SUBJECTS: u32 = MAX_PROPOSAL_SUBJECT_CIDS;

/// 投票引擎类型。业务模块必须静态选择，调用方不得传入或覆盖。
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Encode,
    Decode,
    DecodeWithMemTracking,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum VotingEngineKind {
    /// 机构岗位或个人多签内部投票。
    Internal,
    /// 多个机构岗位主体参与的联合投票。
    Joint,
    /// 普选或机构岗位互选。
    Election,
    /// 代表机构、签署、公投及护宪终审组成的立法投票。
    Legislation,
}

/// 投票计划字段或主体组合不合法。
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Encode,
    Decode,
    DecodeWithMemTracking,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum VotePlanValidationError {
    /// 业务 owner 与业务动作的 module tag 不一致。
    ProposalOwnerMismatch,
    /// 没有任何投票主体。
    VoterSubjectsEmpty,
    /// 投票主体超过协议上限。
    TooManyVoterSubjects,
    /// 同一完整授权主体重复出现。
    DuplicateVoterSubject,
    /// 机构岗位主体与个人多签主体混用，或个人多签账户不一致。
    AuthorizationSubjectMismatch,
}

/// 业务模块在创建提案时绑定的完整投票计划。
///
/// 本类型只冻结跨模块和跨端 SCALE 字段序；本步骤不写入 storage，也不改变现有投票流程。
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(MaxModuleTagLen))]
pub struct VotePlan<AccountId, MaxModuleTagLen>
where
    MaxModuleTagLen: Get<u32>,
{
    /// 业务模块与具体动作的稳定标识。
    pub business_action_id: BusinessActionId<BoundedVec<u8, MaxModuleTagLen>>,
    /// 现有 ProposalOwner 业务模块标签，必须与 business_action_id.module_tag 相等。
    pub proposal_owner: BoundedVec<u8, MaxModuleTagLen>,
    /// 发起提案的机构岗位主体或个人多签主体。
    pub proposer_subject: AuthorizationSubject<CidNumber, RoleCode, AccountId>,
    /// 有资格投票的岗位主体集合；个人多签固定为同一账户一项。
    /// 普选的选民来自提案人口快照，因此 `Election` 普选计划允许为空；互选仍由
    /// election-vote 强制要求至少一个机构岗位主体。
    pub voter_subjects: BoundedVec<
        AuthorizationSubject<CidNumber, RoleCode, AccountId>,
        ConstU32<MAX_VOTE_PLAN_SUBJECTS>,
    >,
    /// 业务模块静态选择的投票引擎。
    pub voting_engine: VotingEngineKind,
    /// 业务对象的 32 字节摘要，防止提案与执行对象错绑。
    pub business_object_hash: [u8; 32],
}

impl<AccountId, MaxModuleTagLen> VotePlan<AccountId, MaxModuleTagLen>
where
    AccountId: PartialEq,
    MaxModuleTagLen: Get<u32>,
{
    /// 构造并校验投票计划，禁止空主体、重复主体和机构/个人多签混用。
    pub fn try_new(
        business_action_id: BusinessActionId<BoundedVec<u8, MaxModuleTagLen>>,
        proposal_owner: BoundedVec<u8, MaxModuleTagLen>,
        proposer_subject: AuthorizationSubject<CidNumber, RoleCode, AccountId>,
        voter_subjects: Vec<AuthorizationSubject<CidNumber, RoleCode, AccountId>>,
        voting_engine: VotingEngineKind,
        business_object_hash: [u8; 32],
    ) -> Result<Self, VotePlanValidationError> {
        if business_action_id.module_tag != proposal_owner {
            return Err(VotePlanValidationError::ProposalOwnerMismatch);
        }
        if voter_subjects.is_empty() && voting_engine != VotingEngineKind::Election {
            return Err(VotePlanValidationError::VoterSubjectsEmpty);
        }
        for (index, subject) in voter_subjects.iter().enumerate() {
            if voter_subjects[..index].contains(subject) {
                return Err(VotePlanValidationError::DuplicateVoterSubject);
            }
        }
        match &proposer_subject {
            AuthorizationSubject::Institution(_) => {
                if voter_subjects
                    .iter()
                    .any(|subject| !matches!(subject, AuthorizationSubject::Institution(_)))
                {
                    return Err(VotePlanValidationError::AuthorizationSubjectMismatch);
                }
            }
            AuthorizationSubject::PersonalMultisig(account) => {
                let same_personal_account = matches!(
                    voter_subjects.first(),
                    Some(AuthorizationSubject::PersonalMultisig(voter_account_id))
                        if voter_account_id == account
                );
                if voter_subjects.len() != 1 || !same_personal_account {
                    return Err(VotePlanValidationError::AuthorizationSubjectMismatch);
                }
            }
        }
        let voter_subjects = voter_subjects
            .try_into()
            .map_err(|_| VotePlanValidationError::TooManyVoterSubjects)?;
        Ok(Self {
            business_action_id,
            proposal_owner,
            proposer_subject,
            voter_subjects,
            voting_engine,
            business_object_hash,
        })
    }
}

#[cfg(test)]
mod vote_plan_tests {
    use super::*;
    use entity_primitives::RoleSubject;

    type MaxModuleTagLen = ConstU32<32>;
    type TestVotePlan = VotePlan<[u8; 32], MaxModuleTagLen>;

    fn bounded(bytes: &[u8]) -> BoundedVec<u8, MaxModuleTagLen> {
        bytes.to_vec().try_into().expect("测试标签长度合法")
    }

    fn institution(
        cid_number: &[u8],
        role_code: &[u8],
    ) -> AuthorizationSubject<CidNumber, RoleCode, [u8; 32]> {
        AuthorizationSubject::Institution(RoleSubject {
            cid_number: cid_number.to_vec().try_into().expect("测试 CID 长度合法"),
            role_code: role_code.to_vec().try_into().expect("测试岗位码长度合法"),
        })
    }

    fn valid_plan() -> TestVotePlan {
        let proposer = institution(b"LN001-NRC0G-944805165-2026", b"COMMITTEE_MEMBER");
        let voters = vec![
            proposer.clone(),
            institution(b"ZS001-PRC0E-016974075-2026", b"COMMITTEE_MEMBER"),
            institution(b"ZS001-PRB08-233384677-2026", b"DIRECTOR"),
        ];
        TestVotePlan::try_new(
            BusinessActionId {
                module_tag: bounded(b"res-iss"),
                action_code: 0,
            },
            bounded(b"res-iss"),
            proposer,
            voters,
            VotingEngineKind::Joint,
            [0xabu8; 32],
        )
        .expect("测试投票计划必须合法")
    }

    #[test]
    fn voting_engine_discriminants_are_stable() {
        assert_eq!(VotingEngineKind::Internal.encode(), vec![0]);
        assert_eq!(VotingEngineKind::Joint.encode(), vec![1]);
        assert_eq!(VotingEngineKind::Election.encode(), vec![2]);
        assert_eq!(VotingEngineKind::Legislation.encode(), vec![3]);
    }

    #[test]
    fn vote_plan_field_order_is_stable() {
        let plan = valid_plan();
        assert_eq!(
            plan.encode(),
            (
                plan.business_action_id.clone(),
                plan.proposal_owner.clone(),
                plan.proposer_subject.clone(),
                plan.voter_subjects.clone(),
                plan.voting_engine,
                plan.business_object_hash,
            )
                .encode()
        );
    }

    #[test]
    fn vote_plan_rejects_invalid_subject_combinations() {
        let proposer = institution(b"LN001-NRC0G-944805165-2026", b"COMMITTEE_MEMBER");
        let action = BusinessActionId {
            module_tag: bounded(b"res-iss"),
            action_code: 0,
        };
        let create = |owner: &[u8], proposer_subject, voter_subjects| {
            TestVotePlan::try_new(
                action.clone(),
                bounded(owner),
                proposer_subject,
                voter_subjects,
                VotingEngineKind::Joint,
                [0xabu8; 32],
            )
        };

        assert!(matches!(
            create(b"wrong", proposer.clone(), vec![proposer.clone()]),
            Err(VotePlanValidationError::ProposalOwnerMismatch)
        ));
        assert!(matches!(
            create(b"res-iss", proposer.clone(), Vec::new()),
            Err(VotePlanValidationError::VoterSubjectsEmpty)
        ));
        assert!(matches!(
            create(
                b"res-iss",
                proposer.clone(),
                vec![proposer.clone(), proposer.clone()],
            ),
            Err(VotePlanValidationError::DuplicateVoterSubject)
        ));
        assert!(matches!(
            create(
                b"res-iss",
                proposer,
                vec![AuthorizationSubject::PersonalMultisig([7u8; 32])],
            ),
            Err(VotePlanValidationError::AuthorizationSubjectMismatch)
        ));
    }

    #[test]
    fn personal_multisig_plan_requires_the_same_single_account() {
        let personal = AuthorizationSubject::PersonalMultisig([7u8; 32]);
        let result = TestVotePlan::try_new(
            BusinessActionId {
                module_tag: bounded(b"multisig"),
                action_code: 0,
            },
            bounded(b"multisig"),
            personal.clone(),
            vec![personal],
            VotingEngineKind::Internal,
            [0xcdu8; 32],
        );
        assert!(result.is_ok());

        let mismatched = TestVotePlan::try_new(
            BusinessActionId {
                module_tag: bounded(b"multisig"),
                action_code: 0,
            },
            bounded(b"multisig"),
            AuthorizationSubject::PersonalMultisig([7u8; 32]),
            vec![AuthorizationSubject::PersonalMultisig([8u8; 32])],
            VotingEngineKind::Internal,
            [0xcdu8; 32],
        );
        assert!(matches!(
            mismatched,
            Err(VotePlanValidationError::AuthorizationSubjectMismatch)
        ));
    }
}

/// 单个提案最多关联的机构 CID 数。联合投票需要覆盖 NRC + PRC + PRB 全体机构。
pub const MAX_PROPOSAL_SUBJECT_CIDS: u32 = 256;

/// 提案关联机构 CID 集合。个人多签没有 CID,该集合必须为空。
pub type ProposalSubjectCidNumbers = BoundedVec<CidNumber, ConstU32<MAX_PROPOSAL_SUBJECT_CIDS>>;

/// 内部投票 pallet 的 stage(单阶段提案)。
pub const STAGE_INTERNAL: u8 = 0;
/// 联合投票 pallet 的内部投票阶段(jointinternal):国家储委会/省储委会/省储行管理员加权投票。
pub const STAGE_JOINT: u8 = 1;
/// 联合投票 pallet 的联合公投阶段(jointreferendum):内部投票阶段未全票通过或超时进入,
/// 由 CID 持有者按 >50% 严格多数投票。
///
/// 注意:这是联合投票的第二阶段,与独立的 election-vote pallet(pallet_index=22)
/// 是两个不同概念。election-vote pallet 用于选举公职人员(普选 + 机构成员互选)。
pub const STAGE_REFERENDUM: u8 = 2;

/// 立法机关代表表决阶段：单机构一段，顺序机构按 `current_body` 逐段推进。
pub const STAGE_LEG_REPRESENTATIVE: u8 = 10;
/// 立法投票强制公投阶段(legislation-vote,特别案/核心修宪):内部全过后强制进入。
/// 与联合投票 `STAGE_REFERENDUM` 区分,阈值为宪法 ≥70% 参与 + ≥70% 赞成。
pub const STAGE_LEG_REFERENDUM: u8 = 11;
/// 立法投票行政签署阶段(legislation-vote,ADR-027,宪法第45/46/100/106条):
/// 非特别案内部全过后,由机构法定代表人签署——市长(市)/省长(省)/总统(国)。
/// 市行政区单院无救济;省行政区/国家否决或超时进入 `STAGE_LEG_OVERRIDE`。特别案不经此阶段。
pub const STAGE_LEG_SIGN: u8 = 12;
/// 立法投票三人会签救济阶段(省行政区/国家):立法院院长 + 参议长 + 众议长共同签署。
/// 三人全签同意 → 生效;任一否决或会签超时 → 否决。
pub const STAGE_LEG_OVERRIDE: u8 = 13;
/// 立法投票护宪大法官终审阶段(ADR-027 修订 2026-06-25,宪法第21条):**仅修宪(tier=宪法)**。
/// 修宪在现有流程(重要案 总统签署后 / 特别案 公投后)通过后,最后进入护宪大法官表决:
/// 7 名护宪大法官中 4 名及以上赞成 → 生效;未获 4 名及以上赞成或 30 天超时 → 否决。
pub const STAGE_LEG_CONSTITUTION_GUARD: u8 = 14;

/// 选举投票普选阶段(election-vote):公民按行政区/机构范围投票选人。
pub const STAGE_ELECTION_POPULAR: u8 = 20;
/// 选举投票互选阶段(election-vote):机构现任成员/管理员在成员快照内互选。
pub const STAGE_ELECTION_MUTUAL: u8 = 21;

pub const STATUS_VOTING: u8 = 0;
pub const STATUS_PASSED: u8 = 1;
pub const STATUS_REJECTED: u8 = 2;
/// 提案已执行完成（终态）。消费模块在业务逻辑成功后推进到该状态。
pub const STATUS_EXECUTED: u8 = 3;
/// 投票通过但业务执行失败（终态）。只由投票引擎在重试耗尽、超时或业务永久失败时写入。
pub const STATUS_EXECUTION_FAILED: u8 = 4;

/// 单个提案最多串联的代表机构数量；当前宪法路线最多两个，预留扩展空间。
pub const MAX_REPRESENTATIVE_BODIES: u32 = 4;

/// 业务模块统一执行结果。
///
/// 业务模块只表达“业务动作执行结果”，不再直接改写提案状态。
/// 投票引擎根据该结果统一维护 PASSED / EXECUTED / EXECUTION_FAILED 状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum ProposalExecutionOutcome {
    /// 不是本模块提案。
    Ignored,
    /// 业务执行成功。
    Executed,
    /// 暂时失败，保留 PASSED 并允许管理员手动重试。
    RetryableFailed,
    /// 确定不可执行，进入 EXECUTION_FAILED 终态。
    FatalFailed,
}

/// 业务模块对 `PASSED` 重试提案是否允许管理员提前取消的决策。
///
/// `MODULE_TAG` 只用于路由识别，不能作为权限凭据；因此取消必须由真正
/// 认领该提案的 callback 显式返回 `Allow`，默认实现一律 `Ignored`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum ProposalCancelDecision {
    /// 不是本模块提案。
    Ignored,
    /// 本模块确认该提案已不可执行，允许进入 EXECUTION_FAILED 终态。
    Allow,
}

/// 内部提案互斥类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum InternalProposalMutexKind {
    /// 普通内部治理事项，允许同账户多个普通事项并行。
    Regular,
    /// 管理员集合变更，同账户下必须独占。
    AdminSetMutationExclusive,
}

/// 提案主体键。机构类主体用 CID,个人多签用账户。
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum ProposalSubject<AccountId> {
    /// 机构类主体:公权、私权、非法人组织等全部以 CID 为唯一身份真源。
    InstitutionCid(CidNumber),
    /// 个人多签主体:个人多签没有 CID,继续以个人多签账户作为主体。
    PersonalAccount(AccountId),
}

/// 同一提案主体下的互斥状态。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct InternalProposalMutexState {
    /// 当前占用管理员集合变更独占锁的提案。
    pub admin_set_mutation_proposal: Option<u64>,
    /// 当前普通活跃提案数量。
    pub regular_active_count: u32,
}

impl InternalProposalMutexState {
    pub(crate) fn is_empty(&self) -> bool {
        self.admin_set_mutation_proposal.is_none() && self.regular_active_count == 0
    }
}

/// proposal_id 到互斥锁的反向绑定，用于终态/阶段切换时释放锁。
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct InternalProposalMutexBinding<AccountId> {
    pub subject: ProposalSubject<AccountId>,
    pub kind: InternalProposalMutexKind,
}

/// 自动执行失败后的统一重试状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct ExecutionRetryState<BlockNumber> {
    /// 已失败的手动执行次数。自动执行失败不计入该次数。
    pub manual_attempts: u8,
    /// 第一次自动执行失败所在区块。
    pub first_auto_failed_at: BlockNumber,
    /// 超过该区块仍未执行成功，则自动转 EXECUTION_FAILED。
    pub retry_deadline: BlockNumber,
    /// 最近一次手动执行尝试所在区块。
    pub last_attempt_at: Option<BlockNumber>,
}

/// 已通过提案等待异步业务回调的状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct PendingExecutionState<BlockNumber> {
    /// 回调返回 DispatchError 的自动失败次数。
    pub attempts: u8,
    /// 下一次允许执行的区块。
    pub next_attempt_at: BlockNumber,
}

/// 事项模块接入联合投票时，统一由投票引擎创建提案。
/// 人口快照、联合签名、投票资格和计票数据只允许在 votingengine/joint-vote 内处理。
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct Proposal<BlockNumber, AccountId> {
    /// 提案类型：内部投票/联合投票
    pub kind: u8,
    /// 当前所处投票阶段：内部/联合/公民
    pub stage: u8,
    /// 当前提案状态：投票中/通过/否决
    pub status: u8,
    /// 仅内部投票使用:机构码。该字段只用于分类、路由和规则判断,不是主体身份真源。
    pub internal_code: Option<InstitutionCode>,
    /// 发起机构唯一 CID。个人多签、公民个人或系统提案为空。
    pub actor_cid_number: Option<CidNumber>,
    /// 只有具体资产账户或个人多签确实参与执行时才存在；不得用作机构身份。
    pub execution_account_id: Option<AccountId>,
    /// 提案影响的机构 CID 集合，不得替代发起机构 `actor_cid_number`。
    pub subject_cid_numbers: ProposalSubjectCidNumbers,
    /// 本阶段起始区块
    pub start: BlockNumber,
    /// 本阶段截止区块（超过则超时）
    pub end: BlockNumber,
}

impl<BlockNumber, AccountId: Clone> Proposal<BlockNumber, AccountId> {
    /// 返回用于活跃上限和互斥锁的提案主体键。
    pub fn subject_keys(&self) -> sp_std::vec::Vec<ProposalSubject<AccountId>> {
        if let Some(cid_number) = self.actor_cid_number.clone() {
            return sp_std::vec![ProposalSubject::InstitutionCid(cid_number)];
        }
        if self.internal_code == Some(PMUL) {
            if let Some(account) = self.execution_account_id.clone() {
                return sp_std::vec![ProposalSubject::PersonalAccount(account)];
            }
        }
        sp_std::vec::Vec::new()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct VoteCountU32 {
    /// 赞成票
    pub yes: u32,
    /// 反对票
    pub no: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct VoteCountU64 {
    /// 赞成票
    pub yes: u64,
    /// 反对票
    pub no: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PendingCleanupStage {
    AdminSnapshots,
    /// 清理按完整岗位主体冻结的投票人快照。
    VoterSnapshots,
    /// 清理按机构冻结的岗位票据总数。
    InstitutionTicketCounts,
    /// 仅派发到提案所属 Track，禁止跨模式空扫所有 sub-pallet。
    TrackData,
    /// 清理大对象存储（ProposalObject + ProposalObjectMeta）。
    ProposalObject,
    /// 清理业务数据（ProposalData + ProposalMeta）和核心数据（Proposals + Tallies）。
    /// 这是清理流程的最后一步，单次完成。
    FinalCleanup,
}

/// 延迟清理 FIFO 中的单个任务。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct ScheduledCleanup<BlockNumber> {
    pub cleanup_at: BlockNumber,
    pub proposal_id: u64,
}

/// 提案辅助元数据（由投票引擎统一存储，替代各业务模块的 ProposalCreatedAt / ProposalPassedAt）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct ProposalMetadata<BlockNumber> {
    /// 提案创建时的区块号
    pub created_at: BlockNumber,
    /// 提案通过时的区块号（未通过时为 None）
    pub passed_at: Option<BlockNumber>,
}

/// 提案对象层元数据：记录统一对象存储的类型、长度与哈希。
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct ProposalObjectMetadata<Hash> {
    /// 对象类型，由业务模块自行定义并在解码时识别。
    pub kind: u8,
    /// 对象字节长度，便于链上/链下快速判断对象规模。
    pub object_len: u32,
    /// 对象内容哈希，用于执行和审计时做一致性校验。
    pub object_hash: Hash,
}

/// 提案展示号(双层 ID 设计)。
///
/// 主键 `proposal_id: u64` 是纯单调全局递增,实质无上限。
/// 展示号则按"年份 + 年内序号"组合,与主键解耦,通过单独的
/// `ProposalDisplayId` storage map 反查。
///
/// 客户端渲染走 `2026-#000123` 类格式;链上和后端在创建提案时同步写入。
/// 改展示格式(比如 `2026Q1-001234` 季度制)只动渲染层,主键和存储不动。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct ProposalDisplayMeta {
    /// 创建年份(UTC 公历)。
    pub year: u16,
    /// 年内序号(每年从 0 重置)。u32 上限 42.9 亿/年,实质无上限。
    pub seq_in_year: u32,
}
