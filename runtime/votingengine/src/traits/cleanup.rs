//! 各投票 mode 的有界分块清理接口。

use frame_support::dispatch::DispatchResult;

/// 单步分块清理结果。`(removed_count, has_remaining)`。
pub type CleanupChunkResult = (u32, bool);

/// internal mode 的 chunked cleanup 入口。
///
/// votingengine 主 crate 维护 `PendingProposalCleanups` 状态机,但 internal mode
/// 自己的 storage(`InternalVotesByTicket` / `InternalTallies` / `InternalThresholdSnapshot`)
/// 住在 sub-pallet,所以清理动作必须通过本 trait 派发。
pub trait InternalCleanupHandler {
    /// 内部提案成功执行后的 mode 侧副作用。
    ///
    /// internal-vote 在这里处理个人多签管理员变更对应的生效阈值；机构阈值
    /// 唯一真源在 public/private entity，不经过本回调改写。
    fn on_internal_proposal_executed(_proposal_id: u64) -> DispatchResult {
        Ok(())
    }

    /// 内部提案进入终态后的 mode 侧清理。
    ///
    /// 个人多签注册/管理员变更被拒绝或执行失败时清理其 pending 阈值。
    fn on_internal_proposal_terminal(_proposal_id: u64, _status: u8) -> DispatchResult {
        Ok(())
    }

    /// 分块清理 InternalVotesByTicket。
    /// 返回 `(removed_this_chunk, has_remaining)`。
    fn cleanup_internal_votes_chunk(proposal_id: u64, limit: u32) -> CleanupChunkResult;

    /// 终态清理:删 InternalTallies + InternalThresholdSnapshot(单步完成,小 storage)。
    fn cleanup_internal_terminal(proposal_id: u64);
}

impl InternalCleanupHandler for () {
    fn cleanup_internal_votes_chunk(_proposal_id: u64, _limit: u32) -> CleanupChunkResult {
        (0, false)
    }

    fn cleanup_internal_terminal(_proposal_id: u64) {}
}

/// joint mode 的 chunked cleanup 入口。
///
/// joint storage(JointVotesByTicket / JointInstitutionTallies / JointVotesByInstitution /
/// JointTallies / ReferendumVotesByCid / ReferendumTallies)
/// 住在 joint-vote pallet,votingengine 主 crate 通过本 trait 派发清理。
pub trait JointCleanupHandler {
    fn cleanup_joint_admin_votes_chunk(proposal_id: u64, limit: u32) -> CleanupChunkResult;
    fn cleanup_joint_institution_votes_chunk(proposal_id: u64, limit: u32) -> CleanupChunkResult;
    fn cleanup_joint_institution_tallies_chunk(proposal_id: u64, limit: u32) -> CleanupChunkResult;
    fn cleanup_referendum_votes_chunk(proposal_id: u64, limit: u32) -> CleanupChunkResult;

    /// 终态清理:删 JointTallies + ReferendumTallies(单步)。
    fn cleanup_joint_terminal(proposal_id: u64);
}

impl JointCleanupHandler for () {
    fn cleanup_joint_admin_votes_chunk(_proposal_id: u64, _limit: u32) -> CleanupChunkResult {
        (0, false)
    }
    fn cleanup_joint_institution_votes_chunk(_proposal_id: u64, _limit: u32) -> CleanupChunkResult {
        (0, false)
    }
    fn cleanup_joint_institution_tallies_chunk(
        _proposal_id: u64,
        _limit: u32,
    ) -> CleanupChunkResult {
        (0, false)
    }
    fn cleanup_referendum_votes_chunk(_proposal_id: u64, _limit: u32) -> CleanupChunkResult {
        (0, false)
    }
    fn cleanup_joint_terminal(_proposal_id: u64) {}
}

/// 立法投票 mode 的 chunked cleanup 入口。
/// legislation-vote 自有账本（RepresentativeVotesByTicket / RepresentativeTallies /
/// LegReferendumVotesByCid 等）住在 sub-pallet，核心通过本 trait 派发清理。
pub trait LegislationCleanupHandler {
    fn cleanup_legislation_representative_votes_chunk(
        proposal_id: u64,
        limit: u32,
    ) -> CleanupChunkResult;
    fn cleanup_legislation_referendum_votes_chunk(
        proposal_id: u64,
        limit: u32,
    ) -> CleanupChunkResult;

    /// 终态清理：删除代表元数据、法律元数据和各计票小 storage（单步）。
    fn cleanup_legislation_terminal(proposal_id: u64);
}

impl LegislationCleanupHandler for () {
    fn cleanup_legislation_representative_votes_chunk(
        _proposal_id: u64,
        _limit: u32,
    ) -> CleanupChunkResult {
        (0, false)
    }
    fn cleanup_legislation_referendum_votes_chunk(
        _proposal_id: u64,
        _limit: u32,
    ) -> CleanupChunkResult {
        (0, false)
    }
    fn cleanup_legislation_terminal(_proposal_id: u64) {}
}

/// election mode 的 chunked cleanup 入口。
pub trait ElectionCleanupHandler {
    fn cleanup_election_votes_chunk(proposal_id: u64, limit: u32) -> CleanupChunkResult;
    fn cleanup_election_voters_chunk(proposal_id: u64, limit: u32) -> CleanupChunkResult;
    fn cleanup_election_tallies_chunk(proposal_id: u64, limit: u32) -> CleanupChunkResult;

    /// 终态清理:删 election meta / candidates / results 等小 storage。
    fn cleanup_election_terminal(proposal_id: u64);
}

impl ElectionCleanupHandler for () {
    fn cleanup_election_votes_chunk(_proposal_id: u64, _limit: u32) -> CleanupChunkResult {
        (0, false)
    }
    fn cleanup_election_voters_chunk(_proposal_id: u64, _limit: u32) -> CleanupChunkResult {
        (0, false)
    }
    fn cleanup_election_tallies_chunk(_proposal_id: u64, _limit: u32) -> CleanupChunkResult {
        (0, false)
    }
    fn cleanup_election_terminal(_proposal_id: u64) {}
}
