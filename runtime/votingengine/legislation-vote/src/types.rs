//! 立法机关表决的稳定强类型协议。
//!
//! 本文件只描述“由哪些代表机构依次表决、使用什么门槛、表决完成后进入什么程序”。
//! 法律正文、任免职书和预算正文归各自业务模块，不进入投票引擎。

use codec::{Decode, Encode, MaxEncodedLen};
use entity_primitives::RoleSubject;
use frame_support::pallet_prelude::{BoundedVec, ConstU32, DecodeWithMemTracking};
use scale_info::TypeInfo;
use sp_runtime::sp_std::vec::Vec;
use sp_runtime::Debug;
use sp_runtime::DispatchError;
use votingengine::types::{CidNumber, ProposalSubjectCidNumbers, RoleCode, VotePlanOf};

/// 单个立法机关表决提案最多串联的代表机构数量。
pub const MAX_REPRESENTATIVE_BODIES: u32 = votingengine::types::MAX_REPRESENTATIVE_BODIES;

/// 参加表决的机构引用。
pub type RepresentativeBody = RoleSubject<CidNumber, RoleCode>;

/// 顺序表决机构列表。
pub type RepresentativeBodies = BoundedVec<RepresentativeBody, ConstU32<MAX_REPRESENTATIVE_BODIES>>;

/// 代表机构表决路线。
///
/// `Sequential` 不命名为“双院”，因为国家教委会→国家立法院参议会同样属于两个机构
/// 顺序表决，但国家教委会不是立法院的一院。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum RepresentativeRoute {
    /// 单个代表机构完成表决。
    Single(RepresentativeBody),
    /// 两个或更多代表机构按声明顺序逐个表决。
    Sequential(RepresentativeBodies),
}

impl RepresentativeRoute {
    /// 返回路线中的全部表决机构，顺序就是状态机推进顺序。
    pub fn bodies(&self) -> Vec<RepresentativeBody> {
        match self {
            Self::Single(body) => Vec::from([body.clone()]),
            Self::Sequential(bodies) => bodies.to_vec(),
        }
    }

    /// 返回表决机构数量。
    pub fn len(&self) -> usize {
        match self {
            Self::Single(_) => 1,
            Self::Sequential(bodies) => bodies.len(),
        }
    }

    /// 路线是否为空。强类型 `Single` 永不为空，`Sequential` 在创建时另行拒绝空值。
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Sequential(bodies) if bodies.is_empty())
    }

    /// 读取指定阶段的表决机构。
    pub fn body(&self, index: u32) -> Option<RepresentativeBody> {
        match self {
            Self::Single(body) if index == 0 => Some(body.clone()),
            Self::Single(_) => None,
            Self::Sequential(bodies) => bodies.get(index as usize).cloned(),
        }
    }
}

/// 代表表决数学门槛。
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum RepresentativeVoteRule {
    /// 常规案：超过 80% 参与且参与者中至少 60% 赞成。
    Regular,
    /// 重要案：超过 90% 参与且参与者中至少 70% 赞成。
    Major,
    /// 特别案代表阶段：全员参与且至少 70% 赞成；之后必须进入公民投票。
    Special,
}

/// 代表表决全部通过后的程序。
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum VoteProcedure {
    /// 代表表决结束即形成业务终局结果，供任免、预算等业务使用。
    RepresentativeOnly,
    /// 继续执行法律专属公投、行政签署、共同签署和护宪终审程序。
    Legislation,
}

/// 法律专属程序创建参数。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct LegislationProcedureConfig {
    /// 行政机构法定代表人岗位主体。
    pub executive: Option<RepresentativeBody>,
    /// 省级、国家级三人会签岗位主体；市级和特别案为空。
    pub override_signers: BoundedVec<RepresentativeBody, ConstU32<3>>,
    /// 是否在法律程序通过后进入护宪大法官终审。
    pub needs_guard: bool,
    /// 修宪终审的护宪大法官岗位主体；非修宪为空。
    pub guard: Option<RepresentativeBody>,
}

/// 业务模块创建立法机关表决的唯一接口。
pub trait LegislationVoteEngine<AccountId> {
    /// 创建代表表决完成即终局的提案，供后续任免和预算业务使用。
    // 表决路由、规则、主体和业务数据均是独立协议字段，接口必须保持逐字段传递。
    #[allow(clippy::too_many_arguments)]
    fn create_representative_vote(
        who: AccountId,
        actor_cid_number: CidNumber,
        vote_plan: VotePlanOf<AccountId>,
        route: RepresentativeRoute,
        rule: RepresentativeVoteRule,
        subject_cid_numbers: ProposalSubjectCidNumbers,
        module_tag: &[u8],
        data: Vec<u8>,
    ) -> Result<u64, DispatchError>;

    /// 创建法律提案，代表表决后按法律专属程序继续推进。
    #[allow(clippy::too_many_arguments)]
    fn create_legislation_vote(
        who: AccountId,
        actor_cid_number: CidNumber,
        vote_plan: VotePlanOf<AccountId>,
        route: RepresentativeRoute,
        rule: RepresentativeVoteRule,
        procedure: LegislationProcedureConfig,
        module_tag: &[u8],
        data: Vec<u8>,
        object_data: Vec<u8>,
    ) -> Result<u64, DispatchError>;

    /// 读取特别案公民投票永久凭据。
    fn referendum_result(proposal_id: u64) -> Option<(u64, u64, u64)>;

    /// 读取修宪护宪终审赞成票数。
    fn guard_review_result(proposal_id: u64) -> Option<u32>;
}

impl<AccountId> LegislationVoteEngine<AccountId> for () {
    fn create_representative_vote(
        _who: AccountId,
        _actor_cid_number: CidNumber,
        _vote_plan: VotePlanOf<AccountId>,
        _route: RepresentativeRoute,
        _rule: RepresentativeVoteRule,
        _subject_cid_numbers: ProposalSubjectCidNumbers,
        _module_tag: &[u8],
        _data: Vec<u8>,
    ) -> Result<u64, DispatchError> {
        Err(DispatchError::Other("LegislationVoteEngineNotConfigured"))
    }

    fn create_legislation_vote(
        _who: AccountId,
        _actor_cid_number: CidNumber,
        _vote_plan: VotePlanOf<AccountId>,
        _route: RepresentativeRoute,
        _rule: RepresentativeVoteRule,
        _procedure: LegislationProcedureConfig,
        _module_tag: &[u8],
        _data: Vec<u8>,
        _object_data: Vec<u8>,
    ) -> Result<u64, DispatchError> {
        Err(DispatchError::Other("LegislationVoteEngineNotConfigured"))
    }

    fn referendum_result(_proposal_id: u64) -> Option<(u64, u64, u64)> {
        None
    }

    fn guard_review_result(_proposal_id: u64) -> Option<u32> {
        None
    }
}
