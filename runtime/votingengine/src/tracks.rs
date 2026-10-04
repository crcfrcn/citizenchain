//! 投票类型（Track）统一路由。
//!
//! 核心引擎只认识提案 `kind`，具体阶段、超时判定、模式账本清理和模式终态副作用
//! 全部由对应 sub-pallet 实现。新增投票类型时只需实现 [`ProposalTrackHandler`]
//! 并加入 Runtime 的递归 tuple，不再扩展核心阶段分支。

use crate::{traits::CleanupChunkResult, Proposal};
use frame_support::dispatch::DispatchResult;
use frame_support::weights::Weight;

/// 单个投票类型的完整生命周期处理器。
pub trait ProposalTrackHandler<BlockNumber, AccountId> {
    /// 当前处理器是否认领该提案类型。
    fn handles(kind: u8) -> bool;

    /// 终结当前类型的超时阶段。
    ///
    /// `None` 表示不认领该类型；`Some(Err(_))` 表示已认领但阶段或状态非法。
    fn finalize_timeout(
        proposal: &Proposal<BlockNumber, AccountId>,
        proposal_id: u64,
    ) -> Option<DispatchResult>;

    /// 在一个有界步骤内清理当前类型自己的大账本。
    fn cleanup_chunk(kind: u8, proposal_id: u64, limit: u32) -> Option<CleanupChunkResult>;

    /// 清理当前类型的小型终态存储。
    fn cleanup_terminal(kind: u8, proposal_id: u64) -> Option<()>;

    /// 当前阶段超时判定的 mode-specific 权重。
    fn timeout_weight(stage: u8) -> Option<Weight>;

    /// 一个有界模式账本清理步骤的权重。
    fn cleanup_chunk_weight(kind: u8, limit: u32) -> Option<Weight>;

    /// 模式终态小存储清理权重。
    fn cleanup_terminal_weight(kind: u8) -> Option<Weight>;

    /// 提案执行成功后的类型侧副作用。
    fn on_proposal_executed(kind: u8, _proposal_id: u64) -> Option<DispatchResult> {
        Self::handles(kind).then_some(Ok(()))
    }

    /// 提案进入终态后的类型侧副作用。
    fn on_proposal_terminal(kind: u8, _proposal_id: u64, _status: u8) -> Option<DispatchResult> {
        Self::handles(kind).then_some(Ok(()))
    }
}

/// Runtime 注册的 Track tuple 统一派发接口。
pub trait ProposalTracks<BlockNumber, AccountId> {
    fn finalize_timeout(
        proposal: &Proposal<BlockNumber, AccountId>,
        proposal_id: u64,
    ) -> Option<DispatchResult>;

    fn cleanup_chunk(kind: u8, proposal_id: u64, limit: u32) -> Option<CleanupChunkResult>;

    fn cleanup_terminal(kind: u8, proposal_id: u64) -> bool;

    fn timeout_weight(kind: u8, stage: u8) -> Option<Weight>;

    fn max_timeout_weight() -> Weight;

    fn cleanup_chunk_weight(kind: u8, limit: u32) -> Option<Weight>;

    fn max_cleanup_chunk_weight(limit: u32) -> Weight;

    fn cleanup_terminal_weight(kind: u8) -> Option<Weight>;

    fn max_cleanup_terminal_weight() -> Weight;

    fn on_proposal_executed(kind: u8, proposal_id: u64) -> Option<DispatchResult>;

    fn on_proposal_terminal(kind: u8, proposal_id: u64, status: u8) -> Option<DispatchResult>;
}

impl<BlockNumber, AccountId> ProposalTracks<BlockNumber, AccountId> for () {
    fn finalize_timeout(
        _proposal: &Proposal<BlockNumber, AccountId>,
        _proposal_id: u64,
    ) -> Option<DispatchResult> {
        None
    }

    fn cleanup_chunk(_kind: u8, _proposal_id: u64, _limit: u32) -> Option<CleanupChunkResult> {
        None
    }

    fn cleanup_terminal(_kind: u8, _proposal_id: u64) -> bool {
        false
    }

    fn timeout_weight(_kind: u8, _stage: u8) -> Option<Weight> {
        None
    }

    fn max_timeout_weight() -> Weight {
        Weight::zero()
    }

    fn cleanup_chunk_weight(_kind: u8, _limit: u32) -> Option<Weight> {
        None
    }

    fn max_cleanup_chunk_weight(_limit: u32) -> Weight {
        Weight::zero()
    }

    fn cleanup_terminal_weight(_kind: u8) -> Option<Weight> {
        None
    }

    fn max_cleanup_terminal_weight() -> Weight {
        Weight::zero()
    }

    fn on_proposal_executed(_kind: u8, _proposal_id: u64) -> Option<DispatchResult> {
        None
    }

    fn on_proposal_terminal(_kind: u8, _proposal_id: u64, _status: u8) -> Option<DispatchResult> {
        None
    }
}

/// 递归 tuple：`(当前 Track, 其余 Track)`。
impl<BlockNumber, AccountId, Head, Tail> ProposalTracks<BlockNumber, AccountId> for (Head, Tail)
where
    Head: ProposalTrackHandler<BlockNumber, AccountId>,
    Tail: ProposalTracks<BlockNumber, AccountId>,
{
    fn finalize_timeout(
        proposal: &Proposal<BlockNumber, AccountId>,
        proposal_id: u64,
    ) -> Option<DispatchResult> {
        Head::finalize_timeout(proposal, proposal_id)
            .or_else(|| Tail::finalize_timeout(proposal, proposal_id))
    }

    fn cleanup_chunk(kind: u8, proposal_id: u64, limit: u32) -> Option<CleanupChunkResult> {
        Head::cleanup_chunk(kind, proposal_id, limit)
            .or_else(|| Tail::cleanup_chunk(kind, proposal_id, limit))
    }

    fn cleanup_terminal(kind: u8, proposal_id: u64) -> bool {
        if Head::cleanup_terminal(kind, proposal_id).is_some() {
            true
        } else {
            Tail::cleanup_terminal(kind, proposal_id)
        }
    }

    fn timeout_weight(kind: u8, stage: u8) -> Option<Weight> {
        if Head::handles(kind) {
            Head::timeout_weight(stage)
        } else {
            Tail::timeout_weight(kind, stage)
        }
    }

    fn max_timeout_weight() -> Weight {
        Head::timeout_weight(u8::MAX)
            .unwrap_or_default()
            .max(Tail::max_timeout_weight())
    }

    fn cleanup_chunk_weight(kind: u8, limit: u32) -> Option<Weight> {
        if Head::handles(kind) {
            Head::cleanup_chunk_weight(kind, limit)
        } else {
            Tail::cleanup_chunk_weight(kind, limit)
        }
    }

    fn max_cleanup_chunk_weight(limit: u32) -> Weight {
        Head::cleanup_chunk_weight(u8::MAX, limit)
            .unwrap_or_default()
            .max(Tail::max_cleanup_chunk_weight(limit))
    }

    fn cleanup_terminal_weight(kind: u8) -> Option<Weight> {
        if Head::handles(kind) {
            Head::cleanup_terminal_weight(kind)
        } else {
            Tail::cleanup_terminal_weight(kind)
        }
    }

    fn max_cleanup_terminal_weight() -> Weight {
        Head::cleanup_terminal_weight(u8::MAX)
            .unwrap_or_default()
            .max(Tail::max_cleanup_terminal_weight())
    }

    fn on_proposal_executed(kind: u8, proposal_id: u64) -> Option<DispatchResult> {
        Head::on_proposal_executed(kind, proposal_id)
            .or_else(|| Tail::on_proposal_executed(kind, proposal_id))
    }

    fn on_proposal_terminal(kind: u8, proposal_id: u64, status: u8) -> Option<DispatchResult> {
        Head::on_proposal_terminal(kind, proposal_id, status)
            .or_else(|| Tail::on_proposal_terminal(kind, proposal_id, status))
    }
}
