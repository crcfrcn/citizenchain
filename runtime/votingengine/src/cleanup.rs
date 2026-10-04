//! 提案延迟清理与公平队列。
//!
//! 终态提案先进入按写入顺序排列的延迟 FIFO；保留 90 天到期后转入就绪 FIFO。
//! 就绪任务每次只执行一个有界步骤，未完成任务回到队尾，避免大提案长期阻塞
//! 后续小提案。模式账本只派发到提案所属 Track。

use crate::pallet::{self, Config};
use crate::{PendingCleanupStage, ScheduledCleanup};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::traits::Saturating;

/// 提案完成后保留天数。
const RETENTION_DAYS: u32 = 90;

fn retention_blocks<T: Config>() -> BlockNumberFor<T> {
    let blocks_per_day: BlockNumberFor<T> = (primitives::pow_const::BLOCKS_PER_DAY as u32).into();
    let days: BlockNumberFor<T> = RETENTION_DAYS.into();
    blocks_per_day.saturating_mul(days)
}

/// 注册 90 天后的延迟清理任务。
pub fn schedule_cleanup<T: Config>(
    proposal_id: u64,
    current_block: BlockNumberFor<T>,
) -> DispatchResult {
    let sequence = pallet::ScheduledCleanupTail::<T>::get();
    let next = sequence
        .checked_add(1)
        .ok_or(pallet::Error::<T>::CleanupQueueSequenceExhausted)?;
    pallet::ScheduledCleanups::<T>::insert(
        sequence,
        ScheduledCleanup {
            cleanup_at: current_block.saturating_add(retention_blocks::<T>()),
            proposal_id,
        },
    );
    pallet::ScheduledCleanupTail::<T>::put(next);
    Ok(())
}

/// 把提案追加到公平就绪 FIFO。
pub(crate) fn enqueue_pending_cleanup<T: Config>(proposal_id: u64) -> DispatchResult {
    let sequence = pallet::PendingCleanupQueueTail::<T>::get();
    let next = sequence
        .checked_add(1)
        .ok_or(pallet::Error::<T>::CleanupQueueSequenceExhausted)?;
    pallet::PendingCleanupQueue::<T>::insert(sequence, proposal_id);
    pallet::PendingCleanupQueueTail::<T>::put(next);
    Ok(())
}

/// 激活已经到期的延迟任务。遇到第一个未来任务即可停止，因为 FIFO 按到期时间有序。
pub fn process_scheduled_cleanups<T: Config>(now: BlockNumberFor<T>, max_weight: Weight) -> Weight {
    let db = T::DbWeight::get();
    let max = T::MaxCleanupActivationsPerBlock::get();
    let mut weight = db.reads(2);
    let mut head = pallet::ScheduledCleanupHead::<T>::get();
    let tail = pallet::ScheduledCleanupTail::<T>::get();
    let item_weight = db.reads_writes(
        u64::from(crate::MAX_PROPOSAL_SUBJECT_CIDS).saturating_add(5),
        u64::from(crate::MAX_PROPOSAL_SUBJECT_CIDS).saturating_add(5),
    );

    for _ in 0..max {
        if weight.saturating_add(item_weight).any_gt(max_weight) {
            break;
        }
        if head >= tail {
            break;
        }
        weight = weight.saturating_add(db.reads(1));
        let Some(task) = pallet::ScheduledCleanups::<T>::get(head) else {
            head = head.saturating_add(1);
            continue;
        };
        if task.cleanup_at > now {
            break;
        }

        let trigger_weight = match trigger_cleanup::<T>(task.proposal_id) {
            Ok(trigger_weight) => trigger_weight,
            Err(_) => break,
        };
        pallet::ScheduledCleanups::<T>::remove(head);
        head = head.saturating_add(1);
        weight = weight
            .saturating_add(trigger_weight)
            .saturating_add(db.writes(1));
    }

    pallet::ScheduledCleanupHead::<T>::put(head);
    weight.saturating_add(db.writes(1))
}

fn trigger_cleanup<T: Config>(proposal_id: u64) -> Result<Weight, DispatchError> {
    let db = T::DbWeight::get();
    let mut weight = db.reads(1);
    // 终态路径通常已经释放活跃名额；这里保留兜底，保证异常中断后仍能收敛。
    if let Some(proposal) = pallet::Proposals::<T>::get(proposal_id) {
        for subject in proposal.subject_keys() {
            crate::limit::remove_active_proposal::<T>(subject, proposal_id);
            weight = weight.saturating_add(db.reads_writes(1, 1));
        }
    }

    weight = weight.saturating_add(db.reads(1));
    if !pallet::PendingProposalCleanups::<T>::contains_key(proposal_id) {
        enqueue_pending_cleanup::<T>(proposal_id)?;
        pallet::PendingProposalCleanups::<T>::insert(
            proposal_id,
            PendingCleanupStage::AdminSnapshots,
        );
        weight = weight.saturating_add(db.reads_writes(1, 3));
    }
    Ok(weight)
}
