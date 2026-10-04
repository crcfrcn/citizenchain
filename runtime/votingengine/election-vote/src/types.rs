//! election-vote 本地类型。
//!
//! 本文件只保存“本次选举的运行态快照”。总统、院长、任期、
//! 候选来源等业务规则由发起业务模块或未来选举法模块解释后传入，
//! election-vote 不把这些规则写成常量真源。

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use votingengine::{
    types::{CidNumber, InstitutionVoteTicket, RoleCode},
    CitizenSubject,
};

/// 选举模式：普选或机构内部互选。
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
pub enum ElectionMode {
    /// 公民按行政区/机构范围投票。
    Popular,
    /// 机构现任成员/管理员在成员快照内投票。
    Mutual,
}

impl ElectionMode {
    pub const fn stage(self) -> u8 {
        match self {
            ElectionMode::Popular => votingengine::STAGE_ELECTION_POPULAR,
            ElectionMode::Mutual => votingengine::STAGE_ELECTION_MUTUAL,
        }
    }
}

/// 创建选举时固化的机构岗位快照。
///
/// 发起机构就是拟任职机构，岗位只使用 entity 已有的 `role_code`；具体选举规则由
/// `runtime/public/` 下的业务模块定义，不在投票引擎另造职位编码或规则编号。
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct ElectionMeta {
    pub mode: ElectionMode,
    /// 普选固定人口作用域；互选为 None，资格来自 VotePlan 岗位任职快照。
    pub population_scope: Option<votingengine::PopulationScope>,
    /// 发起和拟任职机构的唯一 CID；不得再保存第二个目标机构 CID。
    pub actor_cid_number: CidNumber,
    /// 当选后拟任职的 entity 真实岗位码。
    pub role_code: RoleCode,
    pub seat_count: u16,
    /// 任期开始日（自纪元起天数），与 entity 任职字段单位一致。
    pub term_start: u32,
    /// 任期结束日（自纪元起天数），与 entity 任职字段单位一致。
    pub term_end: u32,
}

/// 普选票据：永久 CID 负责去重，值同时冻结完整选民和候选公民主体。
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct PopularElectionVote<AccountId> {
    pub voter_subject: CitizenSubject<AccountId>,
    pub candidate_subject: CitizenSubject<AccountId>,
}

/// 选举事件中的完整选民证据。
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub enum ElectionVoter<AccountId> {
    /// 普选公民主体。
    Citizen(CitizenSubject<AccountId>),
    /// 互选机构岗位票据。
    Institution(InstitutionVoteTicket<AccountId>),
}

/// 选举计票汇总。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct ElectionTally {
    pub casted: u32,
}

/// 当选结果项。
#[derive(
    Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct ElectionWinner<AccountId> {
    pub candidate_subject: CitizenSubject<AccountId>,
    pub votes: u32,
    pub seat_index: u16,
}
