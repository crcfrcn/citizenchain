//! 核心引擎派发到各投票 mode 的超时终结接口。

use frame_support::dispatch::DispatchResult;
use sp_runtime::DispatchError;

/// 内部投票超时结算入口,由 internal-vote pallet 实现。
///
/// votingengine 主 crate 的 `finalize_proposal` extrinsic 与 `on_initialize`
/// 自动结算逻辑遇到 `STAGE_INTERNAL` 时通过本 trait 派发,业务实现住在 sub-pallet。
pub trait InternalProposalFinalizer<BlockNumber, AccountId> {
    fn finalize_internal_timeout(
        proposal: &crate::Proposal<BlockNumber, AccountId>,
        proposal_id: u64,
    ) -> DispatchResult;
}

impl<BlockNumber, AccountId> InternalProposalFinalizer<BlockNumber, AccountId> for () {
    fn finalize_internal_timeout(
        _proposal: &crate::Proposal<BlockNumber, AccountId>,
        _proposal_id: u64,
    ) -> DispatchResult {
        Err(DispatchError::Other(
            "InternalProposalFinalizerNotConfigured",
        ))
    }
}

/// 联合投票超时结算入口。joint-vote sub-pallet 实现。
///
/// 联合投票分两阶段:内部投票阶段(STAGE_JOINT)+ 联合公投阶段(STAGE_REFERENDUM)。
/// votingengine 主 crate 的 finalize 路径根据 stage 选择派发到这两个 fn。
pub trait JointProposalFinalizer<BlockNumber, AccountId> {
    fn finalize_joint_timeout(
        proposal: &crate::Proposal<BlockNumber, AccountId>,
        proposal_id: u64,
    ) -> DispatchResult;

    fn finalize_jointreferendum_timeout(
        proposal: &crate::Proposal<BlockNumber, AccountId>,
        proposal_id: u64,
    ) -> DispatchResult;
}

impl<BlockNumber, AccountId> JointProposalFinalizer<BlockNumber, AccountId> for () {
    fn finalize_joint_timeout(
        _proposal: &crate::Proposal<BlockNumber, AccountId>,
        _proposal_id: u64,
    ) -> DispatchResult {
        Err(DispatchError::Other("JointProposalFinalizerNotConfigured"))
    }

    fn finalize_jointreferendum_timeout(
        _proposal: &crate::Proposal<BlockNumber, AccountId>,
        _proposal_id: u64,
    ) -> DispatchResult {
        Err(DispatchError::Other("JointProposalFinalizerNotConfigured"))
    }
}

// 立法投票(legislation-vote)mode trait(ADR-027)
// 核心 votingengine 按 PROPOSAL_KIND_LEGISLATION / STAGE_LEG_* 分发到这些 trait。
// 内部/联合/选举投票 sub-pallet 逻辑零改动。
/// 立法投票超时结算入口。legislation-vote sub-pallet 实现。
/// 代表机构表决阶段支持单机构和多机构顺序推进，法律专属阶段继续处理公投、签署和护宪终审。
/// + 强制公投(STAGE_LEG_REFERENDUM)+ 行政签署(STAGE_LEG_SIGN)+ 三人会签(STAGE_LEG_OVERRIDE)。
pub trait LegislationProposalFinalizer<BlockNumber, AccountId> {
    fn finalize_legislation_representative_timeout(
        proposal: &crate::Proposal<BlockNumber, AccountId>,
        proposal_id: u64,
    ) -> DispatchResult;

    fn finalize_legislation_referendum_timeout(
        proposal: &crate::Proposal<BlockNumber, AccountId>,
        proposal_id: u64,
    ) -> DispatchResult;

    /// 行政签署阶段超时:市行政区(无 legislature)= 视为通过(PASSED);省行政区/国家 = 退回三人会签。
    fn finalize_legislation_sign_timeout(
        _proposal: &crate::Proposal<BlockNumber, AccountId>,
        _proposal_id: u64,
    ) -> DispatchResult {
        Ok(())
    }

    /// 三人会签阶段超时:法案否决(REJECTED)。
    fn finalize_legislation_override_timeout(
        _proposal: &crate::Proposal<BlockNumber, AccountId>,
        _proposal_id: u64,
    ) -> DispatchResult {
        Ok(())
    }

    /// 护宪大法官终审阶段超时(仅修宪):未获4名及以上赞成 → 法案否决(REJECTED)。
    fn finalize_legislation_guard_timeout(
        _proposal: &crate::Proposal<BlockNumber, AccountId>,
        _proposal_id: u64,
    ) -> DispatchResult {
        Ok(())
    }
}

impl<BlockNumber, AccountId> LegislationProposalFinalizer<BlockNumber, AccountId> for () {
    fn finalize_legislation_representative_timeout(
        _proposal: &crate::Proposal<BlockNumber, AccountId>,
        _proposal_id: u64,
    ) -> DispatchResult {
        Err(DispatchError::Other(
            "LegislationProposalFinalizerNotConfigured",
        ))
    }

    fn finalize_legislation_referendum_timeout(
        _proposal: &crate::Proposal<BlockNumber, AccountId>,
        _proposal_id: u64,
    ) -> DispatchResult {
        Err(DispatchError::Other(
            "LegislationProposalFinalizerNotConfigured",
        ))
    }
}

// 选举投票(election-vote)mode trait
// 核心 votingengine 按 PROPOSAL_KIND_ELECTION / STAGE_ELECTION_* 分发到这些 trait。
// 职位、任期、候选来源等规则不放在核心,只由 election-vote 保存运行态快照。
/// 选举投票超时结算入口。election-vote sub-pallet 实现。
pub trait ElectionProposalFinalizer<BlockNumber, AccountId> {
    fn finalize_election_popular_timeout(
        proposal: &crate::Proposal<BlockNumber, AccountId>,
        proposal_id: u64,
    ) -> DispatchResult;

    fn finalize_election_mutual_timeout(
        proposal: &crate::Proposal<BlockNumber, AccountId>,
        proposal_id: u64,
    ) -> DispatchResult;
}

impl<BlockNumber, AccountId> ElectionProposalFinalizer<BlockNumber, AccountId> for () {
    fn finalize_election_popular_timeout(
        _proposal: &crate::Proposal<BlockNumber, AccountId>,
        _proposal_id: u64,
    ) -> DispatchResult {
        Err(DispatchError::Other(
            "ElectionProposalFinalizerNotConfigured",
        ))
    }

    fn finalize_election_mutual_timeout(
        _proposal: &crate::Proposal<BlockNumber, AccountId>,
        _proposal_id: u64,
    ) -> DispatchResult {
        Err(DispatchError::Other(
            "ElectionProposalFinalizerNotConfigured",
        ))
    }
}
